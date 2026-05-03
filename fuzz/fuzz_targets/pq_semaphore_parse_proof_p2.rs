//! Fuzz `zkmcu_verifier_plonky3::pq_semaphore::parse_proof` (Poseidon2 leg).
//!
//! Same panic-freedom invariant as the bn254/stark targets: the parser
//! must return `Ok` or `Err` on every byte string, never panic / abort /
//! hang. Coverage starts from the canonical Phase-F dual-leg seed at
//! `crates/zkmcu-vectors/data/pq-semaphore-d10-dual/proof_p2.bin` plus
//! pre-mutated copies derived from `mutations.rs` M0–M4.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = zkmcu_verifier_plonky3::pq_semaphore::parse_proof(data);
});
