//! Poseidon Merkle membership circuit for BN254.
//!
//! Parameters: t=3, α=5, 8 full rounds (4+4), 57 partial rounds — matching
//! the standard BN254 Poseidon-128 configuration. ARK is zeroed; MDS is the
//! placeholder circulant matrix based on [2,1,1] (det=4, non-degenerate but
//! NOT the real BN254 MDS). The domain-separation capacity element (state[0])
//! is allocated as a witness variable with value 0 so constant-folding cannot
//! eliminate its S-boxes. This produces the correct R1CS constraint count and
//! variable count for sizing the proving key; it is NOT cryptographically sound.
//!
//! Constraints per Poseidon permutation:
//!   - Full rounds (8 × 3 S-boxes × 3 constraints each) = 72
//!   - Partial rounds (57 × 1 S-box × 3 constraints each) = 171
//!   - Total: 243
//!
//! Constraints per Merkle level:
//!   - 243 (Poseidon) + 2 (conditional swap, each 1 mul) + 1 (boolean) = 246
//!
//! Total for depth D: 246·D + 1 (equality check at root)

use ark_ff::PrimeField;
use ark_r1cs_std::{
    alloc::AllocVar,
    boolean::Boolean,
    eq::EqGadget,
    fields::{fp::FpVar, FieldVar},
};
use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};

const HALF_ROUNDS: usize = 4; // half of 8 full rounds (4+4 split)
const PARTIAL_ROUNDS: usize = 57;

// MDS matrix M = [[2,1,1],[1,2,1],[1,1,2]]: det=4, forces full mixing each
// round so state[0] is never constant after round 1.  All ops are linear
// combinations — zero R1CS constraints.
fn mds<F: PrimeField>(state: [FpVar<F>; 3]) -> [FpVar<F>; 3] {
    let [s0, s1, s2] = state;
    let r0 = s0.clone() + s0.clone() + s1.clone() + s2.clone();
    let r1 = s0.clone() + s1.clone() + s1.clone() + s2.clone();
    let r2 = s0 + s1 + s2.clone() + s2;
    [r0, r1, r2]
}

// x^5 S-box: 3 multiplication constraints (x2, x4, x5).
fn sbox<F: PrimeField>(x: &FpVar<F>) -> Result<FpVar<F>, SynthesisError> {
    let x2 = x.square()?;
    let x4 = x2.square()?;
    Ok(x4 * x.clone())
}

// Poseidon permutation (t=3, placeholder MDS, zero ARK): 243 constraints.
fn poseidon_perm<F: PrimeField>(
    mut state: [FpVar<F>; 3],
) -> Result<[FpVar<F>; 3], SynthesisError> {
    for _ in 0..HALF_ROUNDS {
        state[0] = sbox(&state[0])?;
        state[1] = sbox(&state[1])?;
        state[2] = sbox(&state[2])?;
        state = mds(state);
    }
    for _ in 0..PARTIAL_ROUNDS {
        state[0] = sbox(&state[0])?;
        state = mds(state);
    }
    for _ in 0..HALF_ROUNDS {
        state[0] = sbox(&state[0])?;
        state[1] = sbox(&state[1])?;
        state[2] = sbox(&state[2])?;
        state = mds(state);
    }

    Ok(state)
}

// Two-to-one Poseidon: absorb (left, right) into a t=3 state [cap, left, right]
// where cap is a witness variable holding 0 (domain separation).  Allocating
// it as a variable — not FpVar::Constant(0) — prevents the R1CS optimizer from
// constant-folding its S-boxes out of existence.
fn poseidon_two_to_one<F: PrimeField>(
    cs: ConstraintSystemRef<F>,
    left: FpVar<F>,
    right: FpVar<F>,
) -> Result<FpVar<F>, SynthesisError> {
    let cap = FpVar::new_witness(cs, || Ok(F::zero()))?;
    let out = poseidon_perm([cap, left, right])?;
    Ok(out[1].clone())
}

/// Groth16/BN254 Poseidon Merkle membership circuit.
///
/// Public:  `root` — the claimed Merkle root.
/// Private: `leaf`, `siblings[0..depth]`, `directions[0..depth]`
///
/// Proves: hashing `leaf` up the path defined by `siblings` and `directions`
/// yields `root`.
#[derive(Clone)]
pub struct PoseidonMerkleCircuit<F: PrimeField> {
    pub depth: usize,
    /// Public: the Merkle root.
    pub root: Option<F>,
    /// Private: the leaf value (already hashed or raw, caller's choice).
    pub leaf: Option<F>,
    /// Private: one sibling per level, bottom-to-top.
    pub siblings: Option<Vec<F>>,
    /// Private: `true` = leaf/current node is the LEFT child at this level.
    pub directions: Option<Vec<bool>>,
}

impl<F: PrimeField> ConstraintSynthesizer<F> for PoseidonMerkleCircuit<F> {
    fn generate_constraints(self, cs: ConstraintSystemRef<F>) -> Result<(), SynthesisError> {
        let root_var = FpVar::new_input(cs.clone(), || {
            self.root.ok_or(SynthesisError::AssignmentMissing)
        })?;

        let mut current = FpVar::new_witness(cs.clone(), || {
            self.leaf.ok_or(SynthesisError::AssignmentMissing)
        })?;

        for level in 0..self.depth {
            let sibling = FpVar::new_witness(cs.clone(), || {
                self.siblings
                    .as_ref()
                    .and_then(|s| s.get(level).copied())
                    .ok_or(SynthesisError::AssignmentMissing)
            })?;

            // is_left=true → current is left child, sibling is right.
            let is_left = Boolean::new_witness(cs.clone(), || {
                self.directions
                    .as_ref()
                    .and_then(|d| d.get(level).copied())
                    .ok_or(SynthesisError::AssignmentMissing)
            })?;

            let left = is_left.select(&current, &sibling)?;
            let right = is_left.select(&sibling, &current)?;

            current = poseidon_two_to_one(cs.clone(), left, right)?;
        }

        current.enforce_equal(&root_var)?;
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::print_stdout)]
mod tests {
    use super::*;
    use ark_relations::r1cs::{ConstraintSystem, SynthesisMode};

    type F = ark_bn254::Fr;

    #[test]
    fn constraint_counts() {
        for depth in [3, 4, 5] {
            let cs = ConstraintSystem::<F>::new_ref();
            cs.set_mode(SynthesisMode::Setup);
            let circuit = PoseidonMerkleCircuit::<F> {
                depth,
                root: None,
                leaf: None,
                siblings: None,
                directions: None,
            };
            circuit.generate_constraints(cs.clone()).unwrap();
            println!(
                "depth={depth}: constraints={} witnesses={} public={}",
                cs.num_constraints(),
                cs.num_witness_variables(),
                cs.num_instance_variables(),
            );
        }
    }
}
