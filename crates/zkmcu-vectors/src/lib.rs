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

/// The "square" vector: proves knowledge of `x` such that `x^2 = y`, with `y` public.
///
/// Smallest meaningful Groth16 circuit — one constraint, one public input. Useful as a
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
