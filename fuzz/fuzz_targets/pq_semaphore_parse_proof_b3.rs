//! Fuzz `zkmcu_verifier_plonky3::pq_semaphore_blake3::parse_proof` (Blake3 leg).
//!
//! Sibling of `pq_semaphore_parse_proof_p2`. Same wire-format envelope
//! (postcard) but a different `StarkGenericConfig` pinning Blake3 as the
//! Merkle hash, so the typed shape parsed by `take_from_bytes::<Proof>`
//! is distinct from the Poseidon2 leg's. Coverage starts from
//! `crates/zkmcu-vectors/data/pq-semaphore-d10-dual/proof_b3.bin`.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = zkmcu_verifier_plonky3::pq_semaphore_blake3::parse_proof(data);
});
