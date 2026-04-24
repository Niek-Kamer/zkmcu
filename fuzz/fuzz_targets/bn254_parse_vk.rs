//! Fuzz `zkmcu_verifier::parse_vk` on arbitrary byte sequences.
//!
//! Invariant we're hunting: parse_vk must never panic, abort, loop
//! indefinitely, or return Ok on malformed input. Ok / Err are both
//! fine; a panic is a DoS surface on firmware where `panic-halt` is
//! the strategy. `SECURITY.md`'s threat model requires panic-freedom
//! on adversary-controlled bytes.
//!
//! Seeded from `fuzz/seeds/bn254_parse_vk/` with the committed
//! known-good VK bytes + a handful of adversarial mutations.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = zkmcu_verifier::parse_vk(data);
});
