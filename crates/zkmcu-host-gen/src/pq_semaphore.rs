//! Host-side Plonky3 prover for the PQ-Semaphore AIR.
//!
//! Output:
//! - `crates/zkmcu-vectors/data/pq-semaphore-d10/proof.bin`
//! - `crates/zkmcu-vectors/data/pq-semaphore-d10/public.bin`
//!
//! ## Witness construction
//!
//! Deterministic seeds:
//! - `id` derived from `b"zkmcu-pq-semaphore-v0-id-seed!!!"`
//! - `scope` derived from `b"zkmcu-pq-semaphore-v0-scope-seed"`
//! - `message` derived from `b"zkmcu-pq-sem-v0-message-payload!"`
//!
//! Each 32-byte seed is squeezed into 4 `BabyBear` elements via
//! splitmix-style chunking, giving 4 elements ≈ 124-bit witness space.
//! `signal_hash` is `message` directly (we treat the message as already
//! pre-hashed for v0; downstream callers can swap in a real hasher if
//! needed). Merkle tree: depth 10, 1024 leaves. Leaf 0 is `H(id || 0^12)`,
//! leaves 1..1024 are `H(i || 0^12)` for `i = 1..1023`. The prover claims
//! membership of leaf 0; the path is trivially "all left" (direction
//! bits all zero) and siblings are computed bottom-up.
//!
//! ## Determinism
//!
//! The witness, the trace, and Plonky3's `prove` (non-hiding TwoAdicFriPcs)
//! are all deterministic for fixed seeds + config + AIR. Re-running this
//! function produces byte-identical `proof.bin` and `public.bin`.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::indexing_slicing)]

use std::fs;
use std::path::Path;

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use p3_matrix::dense::RowMajorMatrix;
use p3_uni_stark::prove;
use zkmcu_verifier_plonky3::pq_semaphore::{
    build_air, build_trace_values, build_witness, encode_proof, encode_public_inputs, make_config,
    pack_public_inputs, parse_proof, parse_public_inputs, trace_width, verify_with_config,
    DIGEST_WIDTH,
};
use zkmcu_verifier_plonky3::MAX_PROOF_SIZE;

type Val = BabyBear;

/// Squeeze a 32-byte seed into 4 canonical `BabyBear` elements.
fn seed_to_digest(seed: &[u8; 32]) -> [Val; DIGEST_WIDTH] {
    const BABYBEAR_PRIME: u64 = 0x7800_0001;
    let mut out = [Val::ZERO; DIGEST_WIDTH];
    for (i, slot) in out.iter_mut().enumerate() {
        let mut buf = [0u8; 8];
        buf.copy_from_slice(&seed[i * 8..(i + 1) * 8]);
        let v = u64::from_le_bytes(buf) % BABYBEAR_PRIME;
        *slot = Val::new(v as u32);
    }
    out
}

pub fn run(out_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let dir = out_root.join("pq-semaphore-d10");
    fs::create_dir_all(&dir)?;

    let id_seed: [u8; 32] = *b"zkmcu-pq-semaphore-v0-id-seed!!!";
    let scope_seed: [u8; 32] = *b"zkmcu-pq-semaphore-v0-scope-seed";
    let message_seed: [u8; 32] = *b"zkmcu-pq-sem-v0-message-payload!";

    let id = seed_to_digest(&id_seed);
    let scope = seed_to_digest(&scope_seed);
    let signal_hash = seed_to_digest(&message_seed);

    let witness = build_witness(id, scope, signal_hash);
    let public = pack_public_inputs(&witness);
    let public_bytes = encode_public_inputs(&public);
    let trace_data = build_trace_values(&witness);
    let trace = RowMajorMatrix::new(trace_data, trace_width());

    let air = build_air();
    let config = make_config();
    let proof = prove(&config, &air, trace, &public[..]);
    let proof_bytes = encode_proof(&proof).map_err(|e| format!("encode proof: {e:?}"))?;

    if proof_bytes.len() > MAX_PROOF_SIZE {
        return Err(format!(
            "proof size {} exceeds MAX_PROOF_SIZE {MAX_PROOF_SIZE}",
            proof_bytes.len(),
        )
        .into());
    }

    let parsed_proof = parse_proof(&proof_bytes).map_err(|e| format!("self-parse proof: {e:?}"))?;
    let parsed_public =
        parse_public_inputs(&public_bytes).map_err(|e| format!("self-parse public: {e:?}"))?;
    debug_assert_eq!(parsed_public, public);
    verify_with_config(&parsed_proof, &parsed_public, &config, &air)
        .map_err(|e| format!("self-verify: {e:?}"))?;

    fs::write(dir.join("proof.bin"), &proof_bytes)?;
    fs::write(dir.join("public.bin"), &public_bytes)?;

    println!(
        "wrote pq-semaphore-d10/proof.bin ({} B) + public.bin ({} B)",
        proof_bytes.len(),
        public_bytes.len(),
    );

    Ok(())
}
