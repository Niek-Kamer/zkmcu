//! Fuzz `zkmcu_verifier::parse_public`. Exercises the `count (u32 LE)
//! || Fr[count]` layout, the `MAX_PUBLIC_INPUTS` cap, the strict
//! length check, and substrate-bn's Fr canonical-encoding rejection.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = zkmcu_verifier::parse_public(data);
});
