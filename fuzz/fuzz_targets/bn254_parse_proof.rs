//! Fuzz `zkmcu_verifier::parse_proof`. Same panic-freedom invariant as
//! `bn254_parse_vk`. Fixed-size 256 B wire format so the fuzzer will
//! concentrate on field-element parsing, curve-equation checks, and the
//! G2 subgroup check once the length check is satisfied.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = zkmcu_verifier::parse_proof(data);
});
