//! Fuzz `zkmcu_verifier_plonky3::pq_semaphore::parse_public_inputs`.
//!
//! Both dual-hash legs share the same 64-byte (16 × 4-byte BabyBear)
//! public-inputs blob, so a single fuzz target covers both. Parser
//! invariants under test: exact length match, per-element canonicality
//! check (`< p = 0x78000001`), no panic on any byte string.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = zkmcu_verifier_plonky3::pq_semaphore::parse_public_inputs(data);
});
