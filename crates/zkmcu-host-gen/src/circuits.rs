//! Generic arkworks circuits, curve-agnostic (parameterized over the scalar field).
//!
//! These circuits are deliberately trivial. Their purpose is to produce small,
//! reproducible Groth16 proofs for the embedded verifier to chew on. They are
//! not intended to model anything useful beyond that.

use ark_ff::PrimeField;
use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};

/// Proves knowledge of `x` such that `x * x = y`, where `y` is public.
/// One constraint, one public input. Smallest meaningful Groth16 circuit.
#[derive(Clone)]
pub struct SquareCircuit<F: PrimeField> {
    pub x: Option<F>,
    pub y: Option<F>,
}

impl<F: PrimeField> ConstraintSynthesizer<F> for SquareCircuit<F> {
    fn generate_constraints(self, cs: ConstraintSystemRef<F>) -> Result<(), SynthesisError> {
        use ark_relations::r1cs::LinearCombination;

        let y_var = cs.new_input_variable(|| self.y.ok_or(SynthesisError::AssignmentMissing))?;
        let x_var = cs.new_witness_variable(|| self.x.ok_or(SynthesisError::AssignmentMissing))?;

        cs.enforce_constraint(
            LinearCombination::from(x_var),
            LinearCombination::from(x_var),
            LinearCombination::from(y_var),
        )?;
        Ok(())
    }
}

/// N independent `x_i * x_i = y_i` constraints, each `y_i` public. Used to
/// study how verifier cost scales with public-input count: every additional
/// public input adds one G1 point to the verifying key's IC table and one G1
/// scalar multiplication + point addition to the `vk_x` linear combination
/// during verification.
#[derive(Clone)]
pub struct SquaresNCircuit<F: PrimeField, const N: usize> {
    pub xs: Option<[F; N]>,
    pub ys: Option<[F; N]>,
}

impl<F: PrimeField, const N: usize> ConstraintSynthesizer<F> for SquaresNCircuit<F, N> {
    fn generate_constraints(self, cs: ConstraintSystemRef<F>) -> Result<(), SynthesisError> {
        use ark_relations::r1cs::LinearCombination;

        for i in 0..N {
            let y_var = cs.new_input_variable(|| {
                self.ys
                    .as_ref()
                    .and_then(|ys| ys.get(i).copied())
                    .ok_or(SynthesisError::AssignmentMissing)
            })?;
            let x_var = cs.new_witness_variable(|| {
                self.xs
                    .as_ref()
                    .and_then(|xs| xs.get(i).copied())
                    .ok_or(SynthesisError::AssignmentMissing)
            })?;
            cs.enforce_constraint(
                LinearCombination::from(x_var),
                LinearCombination::from(x_var),
                LinearCombination::from(y_var),
            )?;
        }
        Ok(())
    }
}
