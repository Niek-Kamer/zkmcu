//! Poseidon2-`BabyBear` batched-permutation AIR.
//!
//! Smallest meaningful AIR using the audited Poseidon2-`BabyBear`
//! parameters. Trace columns track each permutation's intermediate
//! state; constraints enforce that every transition matches the
//! audited round structure (`R_F = 8`, `R_P = 13`, `α = 7`, `t = 16`).
//!
//! There are no public inputs — the AIR proves "I correctly applied
//! the audited Poseidon2 permutation N times in parallel". This is
//! Plonky3's reference Poseidon2 hello-world, scaled small enough to
//! fit a microcontroller's heap budget. It is not a useful end-user
//! statement on its own, but it exercises the full Plonky3 verifier
//! stack with the audited round constants in both the AIR-side hash
//! *and* the FRI / Merkle commitment hash, so it makes a clean
//! anchor measurement before the headline PQ-Semaphore AIR lands.
//!
//! See `research/reports/2026-04-29-pq-semaphore-scoping.typ` for the
//! milestone shape.

use alloc::vec::Vec;

use p3_baby_bear::{
    default_babybear_poseidon2_16, BabyBear, GenericPoseidon2LinearLayersBabyBear,
    Poseidon2BabyBear, BABYBEAR_POSEIDON2_HALF_FULL_ROUNDS, BABYBEAR_POSEIDON2_PARTIAL_ROUNDS_16,
    BABYBEAR_POSEIDON2_RC_16_EXTERNAL_FINAL, BABYBEAR_POSEIDON2_RC_16_EXTERNAL_INITIAL,
    BABYBEAR_POSEIDON2_RC_16_INTERNAL, BABYBEAR_S_BOX_DEGREE,
};
use p3_challenger::DuplexChallenger;
use p3_commit::ExtensionMmcs;
use p3_dft::Radix2DitParallel;
use p3_field::extension::BinomialExtensionField;
use p3_field::Field;
use p3_fri::{FriParameters, TwoAdicFriPcs};
use p3_merkle_tree::MerkleTreeMmcs;
use p3_poseidon2_air::{RoundConstants, VectorizedPoseidon2Air};
use p3_symmetric::{PaddingFreeSponge, TruncatedPermutation};
use p3_uni_stark::{verify, StarkConfig};

use crate::Error;

/// State width — width-16 instance, the canonical compression-mode
/// width per the audit (`zkmcu-poseidon-audit::params::STATE_WIDTH`).
pub const WIDTH: usize = 16;

/// S-box exponent. Re-export from `p3-baby-bear` so callers cannot
/// accidentally pass a different value.
pub const SBOX_DEGREE: u64 = BABYBEAR_S_BOX_DEGREE;

/// `VectorizedPoseidon2Air` const generic. One auxiliary register per
/// round captures the raw S-box output before constraint folding;
/// matches the canonical Plonky3 example shape.
pub const SBOX_REGISTERS: usize = 1;

/// Half-external rounds (`R_F / 2`). Audit-locked at 4.
pub const HALF_FULL_ROUNDS: usize = BABYBEAR_POSEIDON2_HALF_FULL_ROUNDS;

/// Partial rounds (`R_P`). Audit-locked at 13.
pub const PARTIAL_ROUNDS: usize = BABYBEAR_POSEIDON2_PARTIAL_ROUNDS_16;

/// Vectorisation factor.
///
/// 1 means each AIR row corresponds to one permutation; the existing
/// Plonky3 reference example uses 8 to amortise commitment work across
/// batches. The on-MCU verifier ships with 1, the smallest setting.
pub const VECTOR_LEN: usize = 1;

type Val = BabyBear;
type Challenge = BinomialExtensionField<Val, 4>;
type Perm = Poseidon2BabyBear<16>;
type FieldHash = PaddingFreeSponge<Perm, 16, 8, 8>;
type Compress = TruncatedPermutation<Perm, 2, 8, 16>;
type ValMmcs =
    MerkleTreeMmcs<<Val as Field>::Packing, <Val as Field>::Packing, FieldHash, Compress, 2, 8>;
type ChallengeMmcs = ExtensionMmcs<Val, Challenge, ValMmcs>;
type Challenger = DuplexChallenger<Val, Perm, 16, 8>;
type Dft = Radix2DitParallel<Val>;
type Pcs = TwoAdicFriPcs<Val, Dft, ValMmcs, ChallengeMmcs>;

/// `StarkConfig` instantiation locked to the audited Poseidon2-`BabyBear`
/// parameters and the chosen FRI shape. Verifier and prover must build
/// the same config; this type is the single source of truth.
pub type Config = StarkConfig<Pcs, Challenge, Challenger>;

/// Concrete proof type for this AIR. Kept as a public alias so callers
/// can name it without re-deriving the `StarkConfig` type spaghetti.
pub type Proof = p3_uni_stark::Proof<Config>;

/// AIR type alias.
pub type Air = VectorizedPoseidon2Air<
    Val,
    GenericPoseidon2LinearLayersBabyBear,
    WIDTH,
    SBOX_DEGREE,
    SBOX_REGISTERS,
    HALF_FULL_ROUNDS,
    PARTIAL_ROUNDS,
    VECTOR_LEN,
>;

// FRI parameters tuned for the smoke-test workload. `NUM_QUERIES = 28`
// keeps the postcard-encoded proof under `MAX_PROOF_SIZE` for the wide
// vectorised trace — see `tests/poseidon_chain.rs` for the headroom math.
// The eventual PQ-Semaphore AIR is much narrower and will fit 64.
const LOG_BLOWUP: usize = 1;
const NUM_QUERIES: usize = 28;
const COMMIT_POW_BITS: usize = 0;
const QUERY_POW_BITS: usize = 0;
const LOG_FINAL_POLY_LEN: usize = 0;
const MAX_LOG_ARITY: usize = 1;

/// Build the AIR with the audited round constants.
#[must_use]
pub const fn build_air() -> Air {
    let constants = RoundConstants::<Val, WIDTH, HALF_FULL_ROUNDS, PARTIAL_ROUNDS>::new(
        BABYBEAR_POSEIDON2_RC_16_EXTERNAL_INITIAL,
        BABYBEAR_POSEIDON2_RC_16_INTERNAL,
        BABYBEAR_POSEIDON2_RC_16_EXTERNAL_FINAL,
    );
    VectorizedPoseidon2Air::new(constants)
}

/// Build a fresh `Config` from the audited Poseidon2-`BabyBear` permutation.
///
/// The cryptographic permutation (used by `MerkleTreeMmcs` for FRI
/// commitments and by `DuplexChallenger` for Fiat-Shamir) is initialised
/// from the audited `BABYBEAR_POSEIDON2_RC_16_*` constants via
/// [`default_babybear_poseidon2_16`]. Verifier and prover must call this
/// same function so the hashes line up; mismatched configs cause `verify`
/// to return a commitment-mismatch error rather than a soundness failure.
#[must_use]
pub fn make_config() -> Config {
    let perm = default_babybear_poseidon2_16();
    let hash = FieldHash::new(perm.clone());
    let compress = Compress::new(perm.clone());
    let val_mmcs = ValMmcs::new(hash, compress, 0);
    let challenge_mmcs = ChallengeMmcs::new(val_mmcs.clone());
    let dft = Dft::default();
    let fri_params = FriParameters {
        log_blowup: LOG_BLOWUP,
        log_final_poly_len: LOG_FINAL_POLY_LEN,
        max_log_arity: MAX_LOG_ARITY,
        num_queries: NUM_QUERIES,
        commit_proof_of_work_bits: COMMIT_POW_BITS,
        query_proof_of_work_bits: QUERY_POW_BITS,
        mmcs: challenge_mmcs,
    };
    let pcs = Pcs::new(dft, val_mmcs, fri_params);
    let challenger = Challenger::new(perm);
    Config::new(pcs, challenger)
}

/// Deserialise a postcard-encoded proof. Caps input length at
/// [`crate::MAX_PROOF_SIZE`] so adversary-supplied bytes cannot drive
/// the decoder into unbounded `Vec` allocation before the size check.
///
/// # Errors
///
/// Returns [`Error::ProofDeserialization`] if the bytes exceed the
/// length cap or if postcard rejects the encoding.
pub fn parse_proof(bytes: &[u8]) -> Result<Proof, Error> {
    if bytes.len() > crate::MAX_PROOF_SIZE {
        return Err(Error::ProofDeserialization);
    }
    let (proof, rest) =
        postcard::take_from_bytes::<Proof>(bytes).map_err(|_| Error::ProofDeserialization)?;
    // Plonky3 / postcard both tolerate trailing bytes; we reject them so
    // two distinct byte sequences cannot decode to the same proof.
    if !rest.is_empty() {
        return Err(Error::ProofDeserialization);
    }
    Ok(proof)
}

/// Verify a parsed proof against a precomputed `Config` and `Air`.
///
/// Variant of [`verify_proof`] that does not allocate a fresh
/// `StarkConfig` per call. Firmware bench loops should build the
/// config once at boot (`make_config()` allocates ~ten `Vec`s for
/// the round constants and FRI parameters) and reuse it across
/// iterations so the timing window measures actual verify work
/// rather than setup churn.
///
/// # Errors
///
/// Returns [`Error::VerificationFailed`] if Plonky3's verifier rejects
/// the proof.
pub fn verify_with_config(proof: &Proof, config: &Config, air: &Air) -> Result<(), Error> {
    verify(config, air, proof, &[]).map_err(|_| Error::VerificationFailed)
}

/// Verify a parsed proof, building a fresh `Config` and `Air` per call.
///
/// # Errors
///
/// Returns [`Error::VerificationFailed`] if Plonky3's verifier rejects
/// the proof (FRI failure, constraint mismatch, OOD evaluation
/// mismatch, etc.).
pub fn verify_proof(proof: &Proof) -> Result<(), Error> {
    verify_with_config(proof, &make_config(), &build_air())
}

/// Convenience: parse and verify in one call.
///
/// # Errors
///
/// Propagates errors from [`parse_proof`] and [`verify_proof`].
pub fn parse_and_verify(bytes: &[u8]) -> Result<(), Error> {
    let proof = parse_proof(bytes)?;
    verify_proof(&proof)
}

/// Encode a proof to a `Vec<u8>` via postcard. Symmetric with
/// [`parse_proof`].
///
/// # Errors
///
/// Returns [`Error::ProofDeserialization`] if postcard rejects the
/// encoding (in practice, only on out-of-memory in the underlying
/// allocator, which the variant name does not strictly describe — the
/// shared error enum keeps the public surface stable across phases).
pub fn encode_proof(proof: &Proof) -> Result<Vec<u8>, Error> {
    postcard::to_allocvec(proof).map_err(|_| Error::ProofDeserialization)
}
