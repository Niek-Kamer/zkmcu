//! End-to-end tests for the PQ-Semaphore AIR.
//!
//! Mirrors `tests/poseidon_chain.rs` but with public inputs (the
//! 64-byte `public.bin` blob) and the custom AIR's witness-construction
//! helpers.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::cast_possible_truncation
)]

use std::time::Instant;

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use p3_matrix::dense::RowMajorMatrix;
use p3_uni_stark::prove;
use zkmcu_verifier_plonky3::pq_semaphore::{
    build_air, build_trace_values, build_witness, encode_proof, encode_public_inputs, make_config,
    pack_public_inputs, parse_and_verify, parse_proof, parse_public_inputs, trace_width,
    verify_with_config, DIGEST_WIDTH, NUM_PUBLIC_INPUTS, PUBLIC_INPUTS_BYTES,
};
use zkmcu_verifier_plonky3::MAX_PROOF_SIZE;

type Val = BabyBear;

const SEED_BYTES: usize = DIGEST_WIDTH * 8;

fn seed_to_digest(seed: &[u8; SEED_BYTES]) -> [Val; DIGEST_WIDTH] {
    const BABYBEAR_PRIME: u64 = 0x7800_0001;
    let mut out = [Val::ZERO; DIGEST_WIDTH];
    for (i, slot) in out.iter_mut().enumerate() {
        let mut buf = [0u8; 8];
        buf.copy_from_slice(&seed[i * 8..(i + 1) * 8]);
        let v = u64::from_le_bytes(buf) % BABYBEAR_PRIME;
        *slot = Val::new(v as u32);
    }
    out
}

const fn make_seed(prefix: &[u8]) -> [u8; SEED_BYTES] {
    let mut out = [b'!'; SEED_BYTES];
    let len = if prefix.len() < SEED_BYTES {
        prefix.len()
    } else {
        SEED_BYTES
    };
    let mut i = 0;
    while i < len {
        out[i] = prefix[i];
        i += 1;
    }
    out
}

// Distinct in-test seeds so a regression in the committed-vector
// path doesn't silently mask a regression here, and vice versa.
const RT_ID: [u8; SEED_BYTES] = make_seed(b"test-pq-semaphore-roundtrip-id-d6");
const RT_SCOPE: [u8; SEED_BYTES] = make_seed(b"test-pq-semaphore-roundtrip-sc-d6");
const RT_SIGNAL: [u8; SEED_BYTES] = make_seed(b"test-pq-semaphore-rt-message-d6");

#[test]
fn pq_semaphore_roundtrip() {
    let id = seed_to_digest(&RT_ID);
    let scope = seed_to_digest(&RT_SCOPE);
    let signal = seed_to_digest(&RT_SIGNAL);

    let witness = build_witness(id, scope, signal);
    let public = pack_public_inputs(&witness);
    let public_bytes = encode_public_inputs(&public);
    let trace = RowMajorMatrix::new(build_trace_values(&witness), trace_width());

    let air = build_air();
    let config = make_config();
    let proof = prove(&config, &air, trace, &public[..]);
    let proof_bytes = encode_proof(&proof).expect("encode proof");
    println!(
        "pq-semaphore roundtrip proof size: {} bytes (public: {} bytes)",
        proof_bytes.len(),
        public_bytes.len(),
    );
    assert!(proof_bytes.len() <= MAX_PROOF_SIZE);
    assert_eq!(public_bytes.len(), PUBLIC_INPUTS_BYTES);

    let parsed_proof = parse_proof(&proof_bytes).expect("parse proof");
    let parsed_public = parse_public_inputs(&public_bytes).expect("parse public");
    assert_eq!(parsed_public, public);

    let start = Instant::now();
    verify_with_config(&parsed_proof, &parsed_public, &config, &air).expect("verify");
    let elapsed = start.elapsed();
    println!("pq-semaphore verify host time: {elapsed:?}");

    parse_and_verify(&proof_bytes, &public_bytes).expect("parse_and_verify roundtrip");
}

#[test]
fn parse_public_rejects_oversize() {
    let bytes = vec![0u8; PUBLIC_INPUTS_BYTES + 1];
    assert!(parse_public_inputs(&bytes).is_err());
}

#[test]
fn parse_public_rejects_undersize() {
    let bytes = vec![0u8; PUBLIC_INPUTS_BYTES - 1];
    assert!(parse_public_inputs(&bytes).is_err());
}

#[test]
fn parse_public_rejects_noncanonical() {
    // BabyBear prime is 0x78000001 = 2_013_265_921.
    // 0x78000001 itself is the smallest non-canonical encoding.
    let mut bytes = vec![0u8; PUBLIC_INPUTS_BYTES];
    bytes[0..4].copy_from_slice(&0x7800_0001u32.to_le_bytes());
    assert!(parse_public_inputs(&bytes).is_err());

    // Confirm a max-canonical value at the same slot is accepted.
    let mut ok_bytes = vec![0u8; PUBLIC_INPUTS_BYTES];
    ok_bytes[0..4].copy_from_slice(&0x7800_0000u32.to_le_bytes());
    assert!(parse_public_inputs(&ok_bytes).is_ok());
}

#[test]
fn parse_proof_rejects_oversize_input() {
    let bytes = vec![0u8; MAX_PROOF_SIZE + 1];
    assert!(parse_proof(&bytes).is_err());
}

/// Load the committed `proof.bin` + `public.bin` from
/// `crates/zkmcu-vectors/data/pq-semaphore-d10/` and verify them through
/// the public API. Same defence-in-depth pattern as the
/// `committed_vector_verifies` test in `poseidon_chain.rs`.
#[test]
fn committed_vector_verifies() {
    static COMMITTED_PROOF: &[u8] =
        include_bytes!("../../zkmcu-vectors/data/pq-semaphore-d10/proof.bin");
    static COMMITTED_PUBLIC: &[u8] =
        include_bytes!("../../zkmcu-vectors/data/pq-semaphore-d10/public.bin");
    parse_and_verify(COMMITTED_PROOF, COMMITTED_PUBLIC).expect("committed vector verifies");
    assert_eq!(COMMITTED_PUBLIC.len(), PUBLIC_INPUTS_BYTES);
    let parsed = parse_public_inputs(COMMITTED_PUBLIC).expect("parse public");
    assert_eq!(parsed.len(), NUM_PUBLIC_INPUTS);
}
