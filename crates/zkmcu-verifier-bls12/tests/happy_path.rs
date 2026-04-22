//! Happy-path + minimum-viable rejection tests. Full adversarial parity with
//! `zkmcu-verifier/tests/adversarial.rs` is Phase 2.3 work; this file exists
//! so Phase 2.2 has a landing test that proves end-to-end byte → verify round-
//! trips on committed vectors.

#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]

use zkmcu_verifier_bls12::{parse_proof, parse_public, parse_vk, verify_bytes, Error};

static SQUARE_VK: &[u8] = include_bytes!("../../zkmcu-vectors/data/bls12-381/square/vk.bin");
static SQUARE_PROOF: &[u8] = include_bytes!("../../zkmcu-vectors/data/bls12-381/square/proof.bin");
static SQUARE_PUBLIC: &[u8] =
    include_bytes!("../../zkmcu-vectors/data/bls12-381/square/public.bin");

static SQUARES_5_VK: &[u8] = include_bytes!("../../zkmcu-vectors/data/bls12-381/squares-5/vk.bin");
static SQUARES_5_PROOF: &[u8] =
    include_bytes!("../../zkmcu-vectors/data/bls12-381/squares-5/proof.bin");
static SQUARES_5_PUBLIC: &[u8] =
    include_bytes!("../../zkmcu-vectors/data/bls12-381/squares-5/public.bin");

#[test]
fn square_vector_verifies() {
    assert_eq!(
        verify_bytes(SQUARE_VK, SQUARE_PROOF, SQUARE_PUBLIC),
        Ok(true)
    );
}

#[test]
fn squares_5_vector_verifies() {
    assert_eq!(
        verify_bytes(SQUARES_5_VK, SQUARES_5_PROOF, SQUARES_5_PUBLIC),
        Ok(true)
    );
}

#[test]
fn tampered_proof_does_not_verify() {
    // Flip one bit in the A field element of the proof. The pairing check
    // must reject.
    let mut bad = SQUARE_PROOF.to_vec();
    bad[32] ^= 0x01;
    // Any of these outcomes is acceptable — the proof must not verify as true:
    //   Ok(false): pairing check rejected
    //   InvalidG1: bit flip landed somewhere that breaks curve membership
    //   InvalidFp: bit flip landed inside the 16-byte leading zero padding
    match verify_bytes(SQUARE_VK, &bad, SQUARE_PUBLIC) {
        Ok(false) | Err(Error::InvalidG1 | Error::InvalidFp) => {}
        other => panic!("expected rejection, got {other:?}"),
    }
}

#[test]
fn tampered_public_input_does_not_verify() {
    // Change the public input value. The proof binds to a specific y = x²;
    // any other value must fail the pairing check.
    let mut bad = SQUARE_PUBLIC.to_vec();
    let last = bad.len() - 1;
    bad[last] ^= 0x01;
    assert_eq!(
        verify_bytes(SQUARE_VK, SQUARE_PROOF, &bad),
        Ok(false),
        "swapped public input should not verify"
    );
}

#[test]
fn parse_vk_rejects_astronomical_num_ic() {
    // Craft a VK header that parses G1/G2 then claims num_ic = u32::MAX.
    // The parser must reject via TruncatedInput *before* allocating.
    let mut bad = SQUARE_VK[..G1_SIZE + 3 * G2_SIZE].to_vec();
    bad.extend_from_slice(&u32::MAX.to_le_bytes());
    // Intentionally do NOT extend with any ic bytes.
    let err = parse_vk(&bad).unwrap_err();
    assert_eq!(err, Error::TruncatedInput);
}

#[test]
fn parse_public_rejects_astronomical_count() {
    let mut bad = Vec::new();
    bad.extend_from_slice(&u32::MAX.to_le_bytes());
    // No Fr bytes follow.
    let err = parse_public(&bad).unwrap_err();
    assert_eq!(err, Error::TruncatedInput);
}

#[test]
fn parse_proof_short_buffer_is_truncated() {
    let err = parse_proof(&SQUARE_PROOF[..100]).unwrap_err();
    assert_eq!(err, Error::TruncatedInput);
}

#[test]
fn parse_vk_rejects_fp_padding_bit_flip() {
    // Flip a bit inside the 16-byte leading-zero padding of alpha.x. The
    // strip_fp check must reject.
    let mut bad = SQUARE_VK.to_vec();
    bad[5] ^= 0x01;
    let err = parse_vk(&bad).unwrap_err();
    assert_eq!(err, Error::InvalidFp);
}

// Constants local to this test file so we don't have to re-export them.
// Kept in sync with zkmcu_verifier_bls12 wire format.
const FP_SIZE: usize = 64;
const G1_SIZE: usize = FP_SIZE * 2;
const G2_SIZE: usize = FP_SIZE * 4;
