#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::print_stdout
)]

use std::time::Instant;

use p3_uni_stark::prove;
use zkmcu_verifier_plonky3::poseidon2_chain::{
    build_air, encode_proof, make_config, parse_and_verify, parse_proof, verify_proof, VECTOR_LEN,
};
use zkmcu_verifier_plonky3::MAX_PROOF_SIZE;

const NUM_ROWS: usize = 1 << 6;
const NUM_PERMUTATIONS: usize = NUM_ROWS * VECTOR_LEN;

#[test]
fn poseidon2_babybear_chain_roundtrip() {
    let air = build_air();
    let config = make_config();
    let trace = air.generate_vectorized_trace_rows(NUM_PERMUTATIONS, 1);

    let proof = prove(&config, &air, trace, &[]);

    let proof_bytes = encode_proof(&proof).expect("encode proof");
    let proof_size = proof_bytes.len();
    println!("poseidon2 chain proof size: {proof_size} bytes");
    assert!(
        proof_size <= MAX_PROOF_SIZE,
        "proof size {proof_size} exceeds MAX_PROOF_SIZE {MAX_PROOF_SIZE}",
    );

    let parsed = parse_proof(&proof_bytes).expect("parse proof");

    let start = Instant::now();
    verify_proof(&parsed).expect("verification failed");
    let verify_elapsed = start.elapsed();
    println!("poseidon2 chain verify host time: {verify_elapsed:?}");

    parse_and_verify(&proof_bytes).expect("parse_and_verify roundtrip");
}

#[test]
fn parse_proof_rejects_oversize_input() {
    let bytes = vec![0u8; MAX_PROOF_SIZE + 1];
    assert!(parse_proof(&bytes).is_err());
}

#[test]
fn parse_proof_rejects_trailing_bytes() {
    let air = build_air();
    let config = make_config();
    let trace = air.generate_vectorized_trace_rows(NUM_PERMUTATIONS, 1);
    let proof = prove(&config, &air, trace, &[]);
    let mut bytes = encode_proof(&proof).expect("encode proof");
    bytes.push(0xff);
    assert!(parse_proof(&bytes).is_err());
}

/// Load the committed proof bytes from `crates/zkmcu-vectors/data/` and
/// verify them through the public API. This is the third independent
/// verification of the same bytes — the host generator self-verifies
/// before writing, this test re-verifies on load, the firmware will
/// re-verify on-MCU. Any committed `proof.bin` that fails this test is
/// a regen-vectors regression that must not land.
#[test]
fn committed_vector_verifies() {
    static COMMITTED_PROOF: &[u8] =
        include_bytes!("../../zkmcu-vectors/data/p3-poseidon2-chain-bb/proof.bin");
    parse_and_verify(COMMITTED_PROOF).expect("committed vector verifies");
}
