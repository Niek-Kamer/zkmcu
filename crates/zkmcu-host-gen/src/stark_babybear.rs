//! STARK Fibonacci proof generation over `BabyBear` with `FieldExtension::Quartic`.
//!
//! Companion to [`crate::stark`] wich runs on Goldilocks + Quadratic. The AIR
//! definition lives in [`zkmcu_verifier_stark::fibonacci_babybear`]; both the
//! host-side prover here and the embedded-side verifier import it from the
//! same source. Output goes to `zkmcu-vectors/data/stark-fib-1024-babybear/
//! {proof,public}.bin` so it sits alongside (not on top of) the Goldilocks
//! vectors.
//!
//! `FieldExtension::Quartic` is the variant added by the
//! `Niek-Kamer/winterfell` fork (`vendor/winterfell`). Running this module
//! exercises the entire fork end-to-end: `ExtensibleField<4>`,
//! `QuartExtension<B>`, the enum variant, the prover's Quartic match arm,
//! and the verifier's Quartic match arm.

use std::fs;
use std::path::Path;

use winterfell::crypto::hashers::Blake3_256;
use winterfell::crypto::{DefaultRandomCoin, MerkleTree};
use winterfell::math::FieldElement;
use winterfell::matrix::ColMatrix;
use winterfell::{
    AuxRandElements, BatchingMethod, CompositionPoly, CompositionPolyTrace,
    ConstraintCompositionCoefficients, DefaultConstraintCommitment, DefaultConstraintEvaluator,
    DefaultTraceLde, FieldExtension, PartitionOptions, ProofOptions, Prover, StarkDomain, Trace,
    TraceInfo, TracePolyTable, TraceTable,
};
use zkmcu_babybear::BaseElement;
use zkmcu_verifier_stark::fibonacci_babybear::{FibAir, PublicInputs, PUBLIC_SIZE};

/// Build a length-`n` Fibonacci trace matching the `FibAir` contract.
///
/// Column 0 holds `Fib(2i+1)`, column 1 holds `Fib(2i+2)`, all reduced
/// mod `p = 15 * 2^27 + 1`. At step `n-1` column 1 holds `Fib(2n) mod p`,
/// wich is the value we assert as the public output.
#[allow(clippy::indexing_slicing)]
fn build_fibonacci_trace(n: usize) -> TraceTable<BaseElement> {
    let trace_width = 2;
    let mut trace = TraceTable::new(trace_width, n);
    trace.fill(
        |state| {
            state[0] = BaseElement::ONE;
            state[1] = BaseElement::ONE;
        },
        |_, state| {
            let new_s0 = state[0] + state[1];
            let new_s1 = state[1] + new_s0;
            state[0] = new_s0;
            state[1] = new_s1;
        },
    );
    trace
}

struct FibProver {
    options: ProofOptions,
}

impl Prover for FibProver {
    type BaseField = BaseElement;
    type Air = FibAir;
    type Trace = TraceTable<BaseElement>;
    type HashFn = Blake3_256<BaseElement>;
    type VC = MerkleTree<Self::HashFn>;
    type RandomCoin = DefaultRandomCoin<Self::HashFn>;
    type TraceLde<E: FieldElement<BaseField = Self::BaseField>> =
        DefaultTraceLde<E, Self::HashFn, Self::VC>;
    type ConstraintCommitment<E: FieldElement<BaseField = Self::BaseField>> =
        DefaultConstraintCommitment<E, Self::HashFn, Self::VC>;
    type ConstraintEvaluator<'a, E: FieldElement<BaseField = Self::BaseField>> =
        DefaultConstraintEvaluator<'a, FibAir, E>;

    fn get_pub_inputs(&self, trace: &Self::Trace) -> PublicInputs {
        let last_step = trace.length() - 1;
        PublicInputs {
            result: trace.get(1, last_step),
        }
    }

    fn options(&self) -> &ProofOptions {
        &self.options
    }

    fn new_trace_lde<E: FieldElement<BaseField = Self::BaseField>>(
        &self,
        trace_info: &TraceInfo,
        main_trace: &ColMatrix<Self::BaseField>,
        domain: &StarkDomain<Self::BaseField>,
        partition_option: PartitionOptions,
    ) -> (Self::TraceLde<E>, TracePolyTable<E>) {
        DefaultTraceLde::new(trace_info, main_trace, domain, partition_option)
    }

    fn build_constraint_commitment<E: FieldElement<BaseField = Self::BaseField>>(
        &self,
        composition_poly_trace: CompositionPolyTrace<E>,
        num_constraint_composition_columns: usize,
        domain: &StarkDomain<Self::BaseField>,
        partition_options: PartitionOptions,
    ) -> (Self::ConstraintCommitment<E>, CompositionPoly<E>) {
        DefaultConstraintCommitment::new(
            composition_poly_trace,
            num_constraint_composition_columns,
            domain,
            partition_options,
        )
    }

    fn new_evaluator<'a, E: FieldElement<BaseField = Self::BaseField>>(
        &self,
        air: &'a FibAir,
        aux_rand_elements: Option<AuxRandElements<E>>,
        composition_coefficients: ConstraintCompositionCoefficients<E>,
    ) -> Self::ConstraintEvaluator<'a, E> {
        DefaultConstraintEvaluator::new(air, aux_rand_elements, composition_coefficients)
    }
}

pub fn run(out_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let n: usize = 1024;
    let dir = out_root.join("stark-fib-1024-babybear");
    fs::create_dir_all(&dir)?;

    // 95-bit conjectured security at N=1024 via quartic extension over BabyBear.
    // BabyBear has a 31-bit modulus, so Quartic (124 bits of extension) is the
    // smallest variant that can hit the same security target as Phase 3.2's
    // Goldilocks + Quadratic path (128 bits of extension).
    let options = ProofOptions::new(
        32,                      // num queries
        8,                       // blowup factor
        0,                       // grinding factor
        FieldExtension::Quartic, // F_{p^4} for DEEP + FRI + constraint combining
        8,                       // FRI folding factor
        31,                      // FRI max remainder polynomial degree
        BatchingMethod::Linear,
        BatchingMethod::Linear,
    );

    let trace = build_fibonacci_trace(n);
    let last_step = trace.length() - 1;
    let result = trace.get(1, last_step);

    let prover = FibProver { options };
    let proof = prover.prove(trace)?;

    // Self-verify before bytes hit disk, same defensive pattern as the Goldilocks path.
    let public = PublicInputs { result };
    zkmcu_verifier_stark::fibonacci_babybear::verify(proof.clone(), public).map_err(|e| {
        format!("self-verify failed during stark-fib-1024-babybear generation: {e:?}")
    })?;

    let proof_bytes = proof.to_bytes();
    let result_canonical = result.canonical();
    let mut public_bytes = [0u8; PUBLIC_SIZE];
    public_bytes.copy_from_slice(&result_canonical.to_le_bytes());

    fs::write(dir.join("proof.bin"), &proof_bytes)?;
    fs::write(dir.join("public.bin"), public_bytes)?;

    println!(
        "wrote stark-fib-1024-babybear/proof.bin {} B + public.bin {} B (N={n}, Fib(2N)={result_canonical} mod p)",
        proof_bytes.len(),
        public_bytes.len(),
    );

    Ok(())
}
