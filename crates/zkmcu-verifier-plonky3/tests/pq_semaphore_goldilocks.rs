//! Host-side parity test for the Goldilocks × Quadratic flavour of the
//! PQ-Semaphore AIR. Phase D of the 128-bit security plan.
//!
//! Confirms:
//! - the committed `pq-semaphore-d10-gl` proof + public bytes verify under
//!   the in-tree verifier path the firmware will run
//! - flipping a single byte in either the proof or the public input
//!   makes verification reject
//!
//! Sibling to `pq_semaphore_reject.rs` which exercises the `BabyBear`
//! mutation set.

#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]

use zkmcu_verifier_plonky3::pq_semaphore_goldilocks::parse_and_verify;

static GL_PROOF: &[u8] = include_bytes!("../../zkmcu-vectors/data/pq-semaphore-d10-gl/proof.bin");
static GL_PUBLIC: &[u8] = include_bytes!("../../zkmcu-vectors/data/pq-semaphore-d10-gl/public.bin");

#[test]
fn honest_path_accepts() {
    parse_and_verify(GL_PROOF, GL_PUBLIC).expect("honest pq-semaphore-gl proof must verify");
}

#[test]
fn flipped_proof_byte_rejects() {
    let mut proof = GL_PROOF.to_vec();
    let mid = proof.len() - 64; // tail bytes are Merkle hash data, no varints near them
    proof[mid] ^= 0xff;
    assert!(
        parse_and_verify(&proof, GL_PUBLIC).is_err(),
        "flipped proof byte at len-64 must reject"
    );
}

#[test]
fn flipped_public_byte_rejects() {
    let mut public = GL_PUBLIC.to_vec();
    public[0] ^= 0x01; // low-bit flip stays canonical < Goldilocks p
    assert!(
        parse_and_verify(GL_PROOF, &public).is_err(),
        "flipped public byte must reject"
    );
}
