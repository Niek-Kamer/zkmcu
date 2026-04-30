//! PQ-Semaphore verifier with Blake3 commitments — Phase E sibling.
//!
//! Reuses the Phase B [`crate::pq_semaphore::PqSemaphoreAir`] verbatim. Only
//! the `StarkConfig` changes: Merkle commitments, the FRI commit MMCS, and
//! the Fiat-Shamir transcript all run on Blake3 instead of Poseidon2-`BabyBear`.
//! Every other parameter (field, extension, FRI queries, grinding bits,
//! digest width, AIR shape, public-input layout) is identical.
//!
//! ## Why this module exists
//!
//! Phase E.1 of the 128-bit security plan: a "stacked dual-hash" verify that
//! checks two FRI proofs over the same trace under two cryptographically
//! independent hash functions. Soundness compounds — even if Poseidon2-`BabyBear`
//! suffers a cryptanalytic break, a forged proof must also be valid under
//! Blake3 to fool the verifier. This module is the Blake3 leg; the dual
//! wrapper lives in [`crate::pq_semaphore_dual`].
//!
//! ## Type wiring
//!
//! - `FieldHash = SerializingHasher<Blake3>` — turns `BabyBear` field
//!   elements into a byte stream and hashes with Blake3 → `[u8; 32]`.
//! - `Compress = CompressionFunctionFromHasher<Blake3, 2, 32>` — pairs
//!   two `[u8; 32]` digests by concatenating and hashing with Blake3.
//! - `ValMmcs = MerkleTreeMmcs<Val, u8, FieldHash, Compress, 2, 32>` —
//!   binary Merkle tree, leaves are `BabyBear`, digests are 32 bytes.
//!   No SIMD packing on the field side: every leaf hash is one Blake3 call.
//! - `Challenger = SerializingChallenger32<Val, HashChallenger<u8, Blake3, 32>>`
//!   — Fiat-Shamir transcript serialised as bytes, hashed with Blake3.
//!
//! The same FRI parameters as Phase B (LOG_BLOWUP=1, NUM_QUERIES=64,
//! COMMIT_POW_BITS=16, QUERY_POW_BITS=16) hold here.

#![allow(clippy::doc_markdown)]

use alloc::vec::Vec;

use p3_baby_bear::BabyBear;
use p3_blake3::Blake3;
use p3_challenger::{HashChallenger, SerializingChallenger32};
use p3_commit::ExtensionMmcs;
use p3_dft::Radix2DitParallel;
use p3_field::extension::BinomialExtensionField;
use p3_fri::{FriParameters, TwoAdicFriPcs};
use p3_merkle_tree::MerkleTreeMmcs;
use p3_symmetric::{CompressionFunctionFromHasher, SerializingHasher};
use p3_uni_stark::{verify, StarkConfig};

use crate::pq_semaphore::{build_air, PqSemaphoreAir, NUM_PUBLIC_INPUTS};
use crate::Error;

/// Base field — same as Phase B (`BabyBear`).
pub type Val = BabyBear;
type Challenge = BinomialExtensionField<Val, 4>;
type FieldHash = SerializingHasher<Blake3>;
type Compress = CompressionFunctionFromHasher<Blake3, 2, 32>;
type ValMmcs = MerkleTreeMmcs<Val, u8, FieldHash, Compress, 2, 32>;
type ChallengeMmcs = ExtensionMmcs<Val, Challenge, ValMmcs>;
type Challenger = SerializingChallenger32<Val, HashChallenger<u8, Blake3, 32>>;
type Dft = Radix2DitParallel<Val>;
type Pcs = TwoAdicFriPcs<Val, Dft, ValMmcs, ChallengeMmcs>;

/// Blake3-flavoured `StarkConfig` for the PQ-Semaphore AIR.
pub type Config = StarkConfig<Pcs, Challenge, Challenger>;
/// Concrete proof type for the Blake3 leg.
pub type Proof = p3_uni_stark::Proof<Config>;

const LOG_BLOWUP: usize = 1;
const NUM_QUERIES: usize = 64;
const COMMIT_POW_BITS: usize = 16;
const QUERY_POW_BITS: usize = 16;
const LOG_FINAL_POLY_LEN: usize = 0;
const MAX_LOG_ARITY: usize = 1;

/// Build the Blake3-backed `Config`.
#[must_use]
pub fn make_config() -> Config {
    let field_hash = FieldHash::new(Blake3 {});
    let compress = Compress::new(Blake3 {});
    let val_mmcs = ValMmcs::new(field_hash, compress, 0);
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
    let challenger = Challenger::from_hasher(Vec::new(), Blake3 {});
    Config::new(pcs, challenger)
}

/// Re-export of the Phase B AIR builder so callers don't have to import
/// two modules.
#[must_use]
pub const fn build_blake3_air() -> PqSemaphoreAir {
    build_air()
}

/// Deserialise a postcard-encoded Blake3-flavoured proof.
///
/// # Errors
///
/// Returns [`Error::ProofDeserialization`] on length-cap violation, postcard
/// decode failure, or trailing bytes.
pub fn parse_proof(bytes: &[u8]) -> Result<Proof, Error> {
    if bytes.len() > crate::MAX_PROOF_SIZE {
        return Err(Error::ProofDeserialization);
    }
    let (proof, rest) =
        postcard::take_from_bytes::<Proof>(bytes).map_err(|_| Error::ProofDeserialization)?;
    if !rest.is_empty() {
        return Err(Error::ProofDeserialization);
    }
    Ok(proof)
}

/// Encode a Blake3-flavoured proof to bytes via postcard.
///
/// # Errors
///
/// Returns [`Error::ProofDeserialization`] if postcard rejects the encode
/// (in practice only on allocator OOM).
pub fn encode_proof(proof: &Proof) -> Result<Vec<u8>, Error> {
    postcard::to_allocvec(proof).map_err(|_| Error::ProofDeserialization)
}

/// Verify with externally-built `Config` + `Air` + parsed public inputs.
///
/// # Errors
///
/// Returns [`Error::VerificationFailed`] if Plonky3 rejects the proof.
pub fn verify_with_config(
    proof: &Proof,
    public: &[Val; NUM_PUBLIC_INPUTS],
    config: &Config,
    air: &PqSemaphoreAir,
) -> Result<(), Error> {
    verify(config, air, proof, &public[..]).map_err(|_| Error::VerificationFailed)
}

/// Parse + verify a Blake3-flavoured proof from raw bytes.
///
/// # Errors
///
/// Propagates [`Error::ProofDeserialization`], [`Error::PublicDeserialization`],
/// or [`Error::VerificationFailed`].
pub fn parse_and_verify(proof_bytes: &[u8], public_bytes: &[u8]) -> Result<(), Error> {
    let proof = parse_proof(proof_bytes)?;
    let public = crate::pq_semaphore::parse_public_inputs(public_bytes)?;
    let config = make_config();
    let air = build_blake3_air();
    verify_with_config(&proof, &public, &config, &air)
}
