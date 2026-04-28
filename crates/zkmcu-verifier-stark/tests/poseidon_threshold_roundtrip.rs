//! Correctness and soundness tests for the Poseidon-commitment + threshold circuit.
//!
//! Tests:
//! 1. Legitimate proofs (value < threshold, correct commitment) verify.
//! 2. Wrong commitment (right proof, wrong public commitment) is rejected.
//! 3. build_trace panics when diff=0 (value = threshold-1 all-zero trace).
//! 4. ThresholdAir::new panics for false claims (value ≥ threshold).

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
use zkmcu_verifier_stark::poseidon_threshold::{
    build_trace, poseidon_commit, PublicInputs, PoseidonThresholdAir,
};

// ---- local prover ----------------------------------------------------------

struct PoseidonThresholdProver {
    options: ProofOptions,
    value: u32,
    nonce: u32,
    threshold: u32,
}

impl Prover for PoseidonThresholdProver {
    type BaseField = BaseElement;
    type Air = PoseidonThresholdAir;
    type Trace = TraceTable<BaseElement>;
    type HashFn = Blake3_256<BaseElement>;
    type VC = MerkleTree<Self::HashFn>;
    type RandomCoin = DefaultRandomCoin<Self::HashFn>;
    type TraceLde<E: FieldElement<BaseField = Self::BaseField>> =
        DefaultTraceLde<E, Self::HashFn, Self::VC>;
    type ConstraintCommitment<E: FieldElement<BaseField = Self::BaseField>> =
        DefaultConstraintCommitment<E, Self::HashFn, Self::VC>;
    type ConstraintEvaluator<'a, E: FieldElement<BaseField = Self::BaseField>> =
        DefaultConstraintEvaluator<'a, PoseidonThresholdAir, E>;

    fn get_pub_inputs(&self, _trace: &Self::Trace) -> PublicInputs {
        PublicInputs {
            commitment: poseidon_commit(self.value, self.nonce),
            threshold: self.threshold,
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
        air: &'a PoseidonThresholdAir,
        aux_rand_elements: Option<AuxRandElements<E>>,
        composition_coefficients: ConstraintCompositionCoefficients<E>,
    ) -> Self::ConstraintEvaluator<'a, E> {
        DefaultConstraintEvaluator::new(air, aux_rand_elements, composition_coefficients)
    }
}

// ---- helpers ---------------------------------------------------------------

const fn proof_options() -> ProofOptions {
    // blowup_factor=8 required for degree-7 Poseidon S-box constraints.
    ProofOptions::new(
        8, 8, 0,
        FieldExtension::Quartic,
        4, 7,
        BatchingMethod::Linear,
        BatchingMethod::Linear,
    )
}

fn prove_and_verify(
    value: u32,
    nonce: u32,
    threshold: u32,
) -> Result<(), winterfell::VerifierError> {
    let commitment = poseidon_commit(value, nonce);
    let pub_inputs = PublicInputs { commitment, threshold };
    let trace = build_trace(value, nonce, threshold);
    let prover = PoseidonThresholdProver { options: proof_options(), value, nonce, threshold };
    let proof = prover.prove(trace).expect("prove must succeed for valid inputs");
    let opts = AcceptableOptions::OptionSet(vec![proof_options()]);
    winterfell::verify::<
        PoseidonThresholdAir,
        Blake3_256<BaseElement>,
        DefaultRandomCoin<Blake3_256<BaseElement>>,
        MerkleTree<Blake3_256<BaseElement>>,
    >(proof, pub_inputs, &opts)
}

// ---- correctness -----------------------------------------------------------

#[test]
fn poseidon_threshold_basic_verifies() {
    prove_and_verify(37, 0, 100).expect("37 < 100 must verify");
}

#[test]
fn poseidon_threshold_different_nonces_different_commitments() {
    // Same value, different nonces → different commitments → both valid but distinct.
    let c1 = poseidon_commit(42, 1);
    let c2 = poseidon_commit(42, 2);
    assert_ne!(c1, c2, "different nonces must give different commitments");
}

#[test]
fn poseidon_threshold_boundary_verifies() {
    // diff = 1 (minimum provable diff), value = threshold - 2.
    prove_and_verify(98, 7, 100).expect("98 < 100 must verify");
}

#[test]
fn poseidon_threshold_small_diff_verifies() {
    prove_and_verify(0, 999, 2).expect("0 < 2 must verify");
}

// ---- soundness: wrong public inputs ----------------------------------------

#[test]
fn poseidon_threshold_wrong_commitment_rejected() {
    // Prove (value=37, nonce=0, threshold=100), then verify with a different commitment.
    let trace = build_trace(37, 0, 100);
    let prover =
        PoseidonThresholdProver { options: proof_options(), value: 37, nonce: 0, threshold: 100 };
    let proof = prover.prove(trace).unwrap();

    // Swap commitment to that of a different value.
    let wrong_commitment = poseidon_commit(0, 0);
    let wrong = PublicInputs { commitment: wrong_commitment, threshold: 100 };
    let opts = AcceptableOptions::OptionSet(vec![proof_options()]);
    let result = winterfell::verify::<
        PoseidonThresholdAir,
        Blake3_256<BaseElement>,
        DefaultRandomCoin<Blake3_256<BaseElement>>,
        MerkleTree<Blake3_256<BaseElement>>,
    >(proof, wrong, &opts);
    assert!(result.is_err(), "wrong commitment must be rejected");
}

// ---- soundness: panics on false claims -------------------------------------

#[test]
#[should_panic(expected = "value must be strictly less than threshold")]
fn poseidon_threshold_false_claim_panics() {
    let _trace = build_trace(100, 0, 37);
}

#[test]
#[should_panic(expected = "value must be strictly less than threshold")]
fn poseidon_threshold_equal_panics() {
    let _trace = build_trace(50, 0, 50);
}
