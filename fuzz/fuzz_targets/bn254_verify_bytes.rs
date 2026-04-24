//! Fuzz `zkmcu_verifier::verify_bytes`, the single-call entry point.
//!
//! Splits one fuzz input into three buffers using the first two bytes as
//! length prefixes, then feeds them through `verify_bytes`. Hits the
//! parsers AND, when all three parse successfully, the full
//! Miller-loop + final-exp pairing path. Most random inputs short-
//! circuit at parse time; the corpus mutation starting from a known-
//! good seed is what produces the deeper coverage.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Cheap tri-split: first two bytes pick lengths, rest is payload.
    // If the split is malformed we just pass empty buffers, still
    // exercises the parsers' truncation paths.
    let Some((&a_hi, rest)) = data.split_first() else {
        return;
    };
    let Some((&b_hi, payload)) = rest.split_first() else {
        return;
    };
    let a_len = (a_hi as usize) * 4;
    let b_len = (b_hi as usize) * 2;
    let a_len = a_len.min(payload.len());
    let b_len = b_len.min(payload.len().saturating_sub(a_len));

    let (vk, rest) = payload.split_at(a_len);
    let (proof, public) = rest.split_at(b_len);

    let _ = zkmcu_verifier::verify_bytes(vk, proof, public);
});
