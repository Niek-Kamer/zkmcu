//! Host parity tests for the dual-hash (Phase E.1) PQ-Semaphore verifier.
//!
//! Confirms the committed `pq-semaphore-d10-dual` artefacts behave the way
//! firmware will see them:
//! - both legs accept the honest pair
//! - flipping a byte in EITHER leg's proof rejects the dual verify
//! - flipping a public-input byte rejects (transcript desync hits both legs)
//! - each individual leg can stand alone via its own `parse_and_verify`

#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]

use zkmcu_verifier_plonky3::pq_semaphore::parse_and_verify as parse_and_verify_p2;
use zkmcu_verifier_plonky3::pq_semaphore_blake3::parse_and_verify as parse_and_verify_b3;
use zkmcu_verifier_plonky3::pq_semaphore_dual::parse_and_verify as parse_and_verify_dual;

static DUAL_PROOF_P2: &[u8] =
    include_bytes!("../../zkmcu-vectors/data/pq-semaphore-d10-dual/proof_p2.bin");
static DUAL_PROOF_B3: &[u8] =
    include_bytes!("../../zkmcu-vectors/data/pq-semaphore-d10-dual/proof_b3.bin");
static DUAL_PUBLIC: &[u8] =
    include_bytes!("../../zkmcu-vectors/data/pq-semaphore-d10-dual/public.bin");

#[test]
fn dual_honest_accepts() {
    parse_and_verify_dual(DUAL_PROOF_P2, DUAL_PROOF_B3, DUAL_PUBLIC)
        .expect("honest dual proof must verify");
}

#[test]
fn p2_leg_alone_accepts() {
    parse_and_verify_p2(DUAL_PROOF_P2, DUAL_PUBLIC).expect("Poseidon2 leg must verify on its own");
}

#[test]
fn b3_leg_alone_accepts() {
    parse_and_verify_b3(DUAL_PROOF_B3, DUAL_PUBLIC).expect("Blake3 leg must verify on its own");
}

#[test]
fn flipped_p2_proof_rejects_dual() {
    let mut proof = DUAL_PROOF_P2.to_vec();
    let mid = proof.len() - 64;
    proof[mid] ^= 0xff;
    assert!(
        parse_and_verify_dual(&proof, DUAL_PROOF_B3, DUAL_PUBLIC).is_err(),
        "flipped P2-leg byte must reject dual"
    );
}

#[test]
fn flipped_b3_proof_rejects_dual() {
    let mut proof = DUAL_PROOF_B3.to_vec();
    let mid = proof.len() - 64;
    proof[mid] ^= 0xff;
    assert!(
        parse_and_verify_dual(DUAL_PROOF_P2, &proof, DUAL_PUBLIC).is_err(),
        "flipped B3-leg byte must reject dual"
    );
}

#[test]
fn flipped_public_byte_rejects_dual() {
    let mut public = DUAL_PUBLIC.to_vec();
    public[0] ^= 0x01;
    assert!(
        parse_and_verify_dual(DUAL_PROOF_P2, DUAL_PROOF_B3, &public).is_err(),
        "flipped public byte must reject dual"
    );
}
