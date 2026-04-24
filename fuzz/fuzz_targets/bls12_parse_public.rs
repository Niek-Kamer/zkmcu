//! Fuzz `zkmcu_verifier_bls12::parse_public`.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = zkmcu_verifier_bls12::parse_public(data);
});
