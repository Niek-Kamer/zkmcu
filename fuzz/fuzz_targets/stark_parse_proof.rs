//! Fuzz `zkmcu_verifier_stark::parse_proof`.
//!
//! The proptest suite found two panic paths here first, closed via a
//! `sanity_check_proof_header` pre-screen in `src/lib.rs` (see
//! `research/postmortems/2026-04-24-stark-cross-field-panic.typ`).
//! This fuzz target keeps hammering on the same surface: with coverage
//! guidance starting from the committed goldilocks-fib-1024 proof as a
//! seed, libFuzzer should EITHER find a new panic path we missed OR
//! saturate coverage cleanly. The former is a finding; the latter is
//! confidence.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = zkmcu_verifier_stark::parse_proof(data);
});
