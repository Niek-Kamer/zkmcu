//! Dual-hash PQ-Semaphore verifier (Phase E.1).
//!
//! Top-level wrapper that runs the Phase B Poseidon2-`BabyBear` verifier
//! and the [`crate::pq_semaphore_blake3`] Blake3 verifier in sequence over
//! a shared public-input blob. Soundness compounds across the two
//! cryptographically independent hashes: a forged proof must fool both.
//!
//! ## Wire format
//!
//! Three flash-resident slices come from the host generator:
//! `proof_p2.bin`, `proof_b3.bin`, `public.bin`. Firmware passes each as
//! `&'static [u8]`; this module never copies them onto the heap before
//! parsing — the caller's static slice is forwarded straight into the
//! per-config `parse_proof` (which is itself a heap-resident postcard
//! decode of the proof structure).
//!
//! ## Heap shape
//!
//! Both verifiers allocate. To keep peak heap close to a single-proof
//! verify on the embedded target, [`parse_and_verify`] parses + verifies
//! the Poseidon2 leg first, drops it, then parses + verifies the Blake3
//! leg. Peak heap is `max(p2_peak, b3_peak)` plus the shared public-input
//! parse (a 24 × 4-byte fixed array, negligible).

use crate::Error;

/// Verify both legs of a dual-hash proof from raw bytes.
///
/// # Errors
///
/// Returns the first failure encountered: a Poseidon2-leg parse or verify
/// error short-circuits before the Blake3 leg parses. Public-input decode
/// failure is reported once.
#[allow(clippy::similar_names)]
pub fn parse_and_verify(
    proof_p2_bytes: &[u8],
    proof_b3_bytes: &[u8],
    public_bytes: &[u8],
) -> Result<(), Error> {
    let public = crate::pq_semaphore::parse_public_inputs(public_bytes)?;

    {
        let proof = crate::pq_semaphore::parse_proof(proof_p2_bytes)?;
        let config = crate::pq_semaphore::make_config();
        let air = crate::pq_semaphore::build_air();
        crate::pq_semaphore::verify_with_config(&proof, &public, &config, &air)?;
    }

    let proof = crate::pq_semaphore_blake3::parse_proof(proof_b3_bytes)?;
    let config = crate::pq_semaphore_blake3::make_config();
    let air = crate::pq_semaphore_blake3::build_blake3_air();
    crate::pq_semaphore_blake3::verify_with_config(&proof, &public, &config, &air)
}
