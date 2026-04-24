//! Fuzz `zkmcu_verifier_bls12::parse_vk`. EIP-2537 layout: 128 B G1 +
//! three 256 B G2 + num_ic + IC[n], with per-Fp 16-byte leading-zero
//! padding the parser must enforce. Panic-freedom target; the split
//! G1/G2 curve+subgroup error variants give deeper reject surface.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = zkmcu_verifier_bls12::parse_vk(data);
});
