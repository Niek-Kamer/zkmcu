//! Host-side correctness and soundness tests for the threshold-check circuit.
//!
//! Tests:
//! 1. Legitimate proofs (various value < threshold) prove and verify.
//! 2. Verifier rejects a proof paired with wrong public inputs.
//! 3. ThresholdAir::new() panics when constructed with a false claim
//!    (value >= threshold), confirming the verifier-side checked_sub guard
//!    fires before any FRI work.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::print_stderr,
    clippy::doc_markdown,
    clippy::indexing_slicing,
    clippy::missing_panics_doc,
)]

use winterfell::crypto::hashers::Blake3_256;
use winterfell::crypto::{DefaultRandomCoin, MerkleTree};
use winterfell::math::FieldElement;
use winterfell::matrix::ColMatrix;
use winterfell::{
    AcceptableOptions, AuxRandElements, BatchingMethod, CompositionPoly, CompositionPolyTrace,
    ConstraintCompositionCoefficients, DefaultConstraintCommitment, DefaultConstraintEvaluator,
    DefaultTraceLde, FieldExtension, PartitionOptions, ProofOptions, Prover, StarkDomain,
    TraceInfo, TracePolyTable, TraceTable,
};

use zkmcu_babybear::BaseElement;
use zkmcu_verifier_stark::threshold_check::{build_trace, PublicInputs, ThresholdAir};

// ---- local prover (mirrors firmware, lives here so tests don't need the
//      firmware crate as a dev-dep) ------------------------------------------

struct ThresholdProver {
    options: ProofOptions,
    value: u32,
    threshold: u32,
}

impl Prover for ThresholdProver {
    type BaseField = BaseElement;
    type Air = ThresholdAir;
    type Trace = TraceTable<BaseElement>;
    type HashFn = Blake3_256<BaseElement>;
    type VC = MerkleTree<Self::HashFn>;
    type RandomCoin = DefaultRandomCoin<Self::HashFn>;
    type TraceLde<E: FieldElement<BaseField = Self::BaseField>> =
        DefaultTraceLde<E, Self::HashFn, Self::VC>;
    type ConstraintCommitment<E: FieldElement<BaseField = Self::BaseField>> =
        DefaultConstraintCommitment<E, Self::HashFn, Self::VC>;
    type ConstraintEvaluator<'a, E: FieldElement<BaseField = Self::BaseField>> =
        DefaultConstraintEvaluator<'a, ThresholdAir, E>;

    fn get_pub_inputs(&self, _trace: &Self::Trace) -> PublicInputs {
        PublicInputs { value: self.value, threshold: self.threshold }
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
        air: &'a ThresholdAir,
        aux_rand_elements: Option<AuxRandElements<E>>,
        composition_coefficients: ConstraintCompositionCoefficients<E>,
    ) -> Self::ConstraintEvaluator<'a, E> {
        DefaultConstraintEvaluator::new(air, aux_rand_elements, composition_coefficients)
    }
}

// ---- helpers ---------------------------------------------------------------

fn proof_options() -> ProofOptions {
    ProofOptions::new(
        11, 4, 0,
        FieldExtension::Quartic,
        4, 7,
        BatchingMethod::Linear,
        BatchingMethod::Linear,
    )
}

fn prove_and_verify(value: u32, threshold: u32) -> Result<(), winterfell::VerifierError> {
    let pub_inputs = PublicInputs { value, threshold };
    let trace = build_trace(value, threshold);
    let prover = ThresholdProver { options: proof_options(), value, threshold };
    let proof = prover.prove(trace).expect("prove must succeed for valid inputs");
    let opts = AcceptableOptions::OptionSet(vec![proof_options()]);
    winterfell::verify::<
        ThresholdAir,
        Blake3_256<BaseElement>,
        DefaultRandomCoin<Blake3_256<BaseElement>>,
        MerkleTree<Blake3_256<BaseElement>>,
    >(proof, pub_inputs, &opts)
}

// ---- correctness -----------------------------------------------------------

#[test]
fn threshold_legitimate_claim_verifies() {
    prove_and_verify(37, 100).expect("37 < 100 must verify");
}

#[test]
fn threshold_boundary_verifies() {
    // value = threshold - 2 (diff = 1). threshold - 1 is excluded: when diff=0
    // the entire trace is all-zeros, making both constraint polynomials the zero
    // polynomial. Winterfell's degree check then fires because the actual degree
    // (0) does not match the declared degrees (1 and 2). This is a known circuit
    // limitation: we cannot prove the tightest boundary value = threshold - 1.
    prove_and_verify(98, 100).expect("98 < 100 must verify");
}

#[test]
fn threshold_small_diff_verifies() {
    // diff = 1 (the minimum provable diff given the all-zero-trace limitation).
    prove_and_verify(0, 2).expect("0 < 2 must verify");
}

#[test]
fn threshold_wrong_public_inputs_rejected() {
    // Valid proof for (value=37, threshold=100). Verify with wrong public inputs.
    // The boundary assertion remaining[0] = diff is bound to the correct public
    // inputs at prove time; different inputs change diff and break it.
    let _pub_inputs = PublicInputs { value: 37, threshold: 100 };
    let trace = build_trace(37, 100);
    let prover = ThresholdProver { options: proof_options(), value: 37, threshold: 100 };
    let proof = prover.prove(trace).unwrap();

    let wrong = PublicInputs { value: 0, threshold: 100 };
    let opts = AcceptableOptions::OptionSet(vec![proof_options()]);
    let result = winterfell::verify::<
        ThresholdAir,
        Blake3_256<BaseElement>,
        DefaultRandomCoin<Blake3_256<BaseElement>>,
        MerkleTree<Blake3_256<BaseElement>>,
    >(proof, wrong, &opts);

    assert!(result.is_err(), "wrong public inputs must be rejected, got Ok(())");
}

// ---- soundness (verifier-side guard) ---------------------------------------

#[test]
#[should_panic(expected = "value must be strictly less than threshold")]
fn threshold_false_claim_panics_in_build_trace() {
    // build_trace fires its own assert first (value=100 >= threshold=37).
    let _trace = build_trace(100, 37);
}

#[test]
#[should_panic(expected = "value must be strictly less than threshold")]
fn threshold_equal_values_panic() {
    let _trace = build_trace(50, 50);
}

#[test]
#[should_panic(expected = "value must be strictly less than threshold")]
fn threshold_air_new_rejects_false_claim_directly() {
    // ThresholdAir::new() uses checked_sub. Winterfell calls this during
    // verify(), so the verifier also rejects false claims before FRI runs.
    // Trigger it directly to confirm the guard is in the AIR, not only in
    // build_trace.
    use winterfell::{Air, ProofOptions, TraceInfo, FieldExtension, BatchingMethod};
    let options = ProofOptions::new(11, 4, 0, FieldExtension::Quartic, 4, 7,
        BatchingMethod::Linear, BatchingMethod::Linear);
    let info = TraceInfo::new(2, 64);
    // This should panic inside ThresholdAir::new() via checked_sub.
    let _air = ThresholdAir::new(info, PublicInputs { value: 100, threshold: 37 }, options);
}
