//! Fuzz the full dual-leg `parse_and_verify` orchestration.
//!
//! Tri-split framing on the libFuzzer input:
//!
//! ```text
//! [u32 LE len_p2] [proof_p2 bytes] [u32 LE len_b3] [proof_b3 bytes] [public bytes (rest)]
//! ```
//!
//! Truncated / inconsistent frames pass empty buffers through to
//! `parse_and_verify`, which exercises the parsers' truncation paths.
//! Verify-time errors are fine — the property is panic-freedom on any
//! input. Throughput is dominated by the host-side verify path
//! (~10–20 ms per accepted proof); the fuzzer spends the bulk of its
//! cycles in parse rejects, which is the surface we care about.
//!
//! Seed corpus carries a length-prefixed bundle of the canonical
//! `proof_p2.bin / proof_b3.bin / public.bin` triple plus the same
//! triple under each `mutations.rs` M0–M5 perturbation.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some((len_p2, rest)) = read_u32_le(data) else {
        return;
    };
    let len_p2 = (len_p2 as usize).min(rest.len());
    let (proof_p2, rest) = rest.split_at(len_p2);

    let Some((len_b3, rest)) = read_u32_le(rest) else {
        return;
    };
    let len_b3 = (len_b3 as usize).min(rest.len());
    let (proof_b3, public) = rest.split_at(len_b3);

    let _ = zkmcu_verifier_plonky3::pq_semaphore_dual::parse_and_verify(proof_p2, proof_b3, public);
});

fn read_u32_le(data: &[u8]) -> Option<(u32, &[u8])> {
    let (head, rest) = data.split_first_chunk::<4>()?;
    Some((u32::from_le_bytes(*head), rest))
}
