#![no_std]

extern crate alloc;

use alloc::vec::Vec;

use zkmcu_verifier::{parse_proof, parse_public, parse_vk, Error, Fr, Proof, VerifyingKey};

/// A single test vector: a VK, a proof valid under it, and the public inputs it binds to.
pub struct TestVector {
    pub name: &'static str,
    pub vk: VerifyingKey,
    pub proof: Proof,
    pub public: Vec<Fr>,
}

// Populated by `cargo run -p zkmcu-host-gen --release`.
// Checked in as binary files so firmware builds don't require the arkworks toolchain.
static SQUARE_VK: &[u8] = include_bytes!("../data/square/vk.bin");
static SQUARE_PROOF: &[u8] = include_bytes!("../data/square/proof.bin");
static SQUARE_PUBLIC: &[u8] = include_bytes!("../data/square/public.bin");

static SQUARES_5_VK: &[u8] = include_bytes!("../data/squares-5/vk.bin");
static SQUARES_5_PROOF: &[u8] = include_bytes!("../data/squares-5/proof.bin");
static SQUARES_5_PUBLIC: &[u8] = include_bytes!("../data/squares-5/public.bin");

// Imported from the vendored Semaphore fork (vendor/semaphore/...) by
// `cargo run -p zkmcu-host-gen --release -- semaphore --depth 10 --proof ...`.
// Real production Groth16/BN254 trusted setup, not a synthetic test vector.
// 4 public inputs (merkle root, nullifier, hash(message), hash(scope)), IC
// size = nPublic + 1 = 5. See crates/zkmcu-vectors/data/semaphore-depth-10/
// for the bytes themselves and scripts/gen-semaphore-proof/ for the gen
// pipeline.
static SEMAPHORE_DEPTH_10_VK: &[u8] = include_bytes!("../data/semaphore-depth-10/vk.bin");
static SEMAPHORE_DEPTH_10_PROOF: &[u8] = include_bytes!("../data/semaphore-depth-10/proof.bin");
static SEMAPHORE_DEPTH_10_PUBLIC: &[u8] = include_bytes!("../data/semaphore-depth-10/public.bin");

/// The "square" vector: proves knowledge of `x` such that `x^2 = y`, with `y` public.
///
/// Smallest meaningful Groth16 circuit, one constraint, one public input. Useful as a
/// sanity check for the verifier before layering on heavier circuits.
pub fn square() -> Result<TestVector, Error> {
    Ok(TestVector {
        name: "square",
        vk: parse_vk(SQUARE_VK)?,
        proof: parse_proof(SQUARE_PROOF)?,
        public: parse_public(SQUARE_PUBLIC)?,
    })
}

/// The "squares-5" vector: proves knowledge of `x_0..x_4` such that
/// `x_i^2 = y_i` for each of five independent pairs, with all `y_i` public.
///
/// Used to measure how verifier cost scales with the number of public inputs:
/// each additional input adds one G1 point to the verifying key's IC table and
/// one G1 scalar multiplication + point addition to the `vk_x` linear
/// combination computed during verification.
pub fn squares_5() -> Result<TestVector, Error> {
    Ok(TestVector {
        name: "squares-5",
        vk: parse_vk(SQUARES_5_VK)?,
        proof: parse_proof(SQUARES_5_PROOF)?,
        public: parse_public(SQUARES_5_PUBLIC)?,
    })
}

/// Real Semaphore (v4.14.2) Groth16/BN254 proof at Merkle tree depth 10.
///
/// Unlike [`square`] and [`squares_5`] wich use synthetic arkworks-generated
/// trusted setups, this vector comes from the production Semaphore snark-
/// artifacts via snarkjs. Public inputs are the canonical Semaphore
/// 4-tuple: `[merkleTreeRoot, nullifier, hash(message), hash(scope)]`,
/// all full 254-bit scalars.
///
/// Loading this vector at runtime proves end-to-end compatibility with
/// real-world circuits deployed on Ethereum. For circuit designers it's
/// also a more realistic verify-cost benchmark than `squares_5`, since
/// Semaphore's big-scalar public inputs exercise the full G1 scalar-mul
/// path in `vk_x = IC[0] + Σ public[i] · IC[i+1]` rather than substrate-
/// bn's small-scalar shortcut.
pub fn semaphore_depth_10() -> Result<TestVector, Error> {
    Ok(TestVector {
        name: "semaphore-depth-10",
        vk: parse_vk(SEMAPHORE_DEPTH_10_VK)?,
        proof: parse_proof(SEMAPHORE_DEPTH_10_PROOF)?,
        public: parse_public(SEMAPHORE_DEPTH_10_PUBLIC)?,
    })
}

/// UMAAL Known-Answer Test vectors for BN254 Fq multiplication.
///
/// Layout: N records of 96 bytes each, `(a, b, a*b)` with every value a
/// 32-byte big-endian Fq element. Generated on host via substrate-bn's
/// pure-Rust `mul_reduce` path; firmware flashed with the `cortex-m33-asm`
/// feature runs each record through the ARMv8-M UMAAL asm and asserts
/// byte-identical product. Any divergence is a miscompute in the asm (or a
/// toolchain regression) that would otherwise silently cascade into
/// forged-proof acceptance in a Groth16 verify.
pub const UMAAL_KAT: &[u8] = include_bytes!("../data/umaal-kat/kat.bin");

/// Size of one UMAAL KAT record (`a ‖ b ‖ a*b`, 32 B big-endian each).
pub const UMAAL_KAT_RECORD_SIZE: usize = 96;
