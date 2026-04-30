//! Host-side Plonky3 prover for the Goldilocks × Quadratic flavour of the
//! PQ-Semaphore AIR.
//!
//! Output:
//! - `crates/zkmcu-vectors/data/pq-semaphore-d10-gl/proof.bin`
//! - `crates/zkmcu-vectors/data/pq-semaphore-d10-gl/public.bin`
//!
//! Sibling to [`crate::pq_semaphore`] (BabyBear × Quartic). Same depth-10
//! Merkle membership + nullifier + scope-binding statement; the only
//! difference is the field. Phase D of the 128-bit security plan exists
//! to compare BabyBear × Quartic + grinding + d6 (~127 conjectured,
//! portable) against Goldilocks × Quadratic + grinding (~127 conjectured
//! FRI over a 128-bit native field).
//!
//! ## Witness construction
//!
//! Deterministic seeds, each `DIGEST_WIDTH * 8 = 32` bytes wide:
//! - `id` derived from `b"zkmcu-pq-semaphore-v0-id-seed-gl"`
//! - `scope` derived from `b"zkmcu-pq-semaphore-v0-scope-seed-gl"`
//! - `message` derived from `b"zkmcu-pq-sem-v0-message-payload-gl"`
//!
//! Each 32-byte seed is squeezed into 4 Goldilocks elements via
//! splitmix-style chunking. Merkle tree shape is identical to the
//! BabyBear sibling (depth 10, leaf 0 is `H(id || 0)`, leaves 1..1024
//! are `H(i || 0)`); the prover claims membership of leaf 0.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::indexing_slicing)]

use std::fs;
use std::path::Path;

use p3_field::PrimeCharacteristicRing;
use p3_goldilocks::Goldilocks;
use p3_matrix::dense::RowMajorMatrix;
use p3_uni_stark::prove;
use zkmcu_verifier_plonky3::pq_semaphore_goldilocks::{
    build_air, build_trace_values, build_witness, encode_proof, encode_public_inputs, make_config,
    pack_public_inputs, parse_proof, parse_public_inputs, trace_width, verify_with_config,
    DIGEST_WIDTH,
};
use zkmcu_verifier_plonky3::MAX_PROOF_SIZE;

type Val = Goldilocks;

/// Length in bytes required to derive `DIGEST_WIDTH` Goldilocks elements
/// (8 bytes per element, splitmix-style).
const SEED_BYTES: usize = DIGEST_WIDTH * 8;

/// Squeeze an 8 × `DIGEST_WIDTH`-byte seed into `DIGEST_WIDTH` canonical
/// Goldilocks elements.
fn seed_to_digest(seed: &[u8; SEED_BYTES]) -> [Val; DIGEST_WIDTH] {
    const GOLDILOCKS_PRIME: u128 = 0xFFFF_FFFF_0000_0001;
    let mut out = [Val::ZERO; DIGEST_WIDTH];
    for (i, slot) in out.iter_mut().enumerate() {
        let mut buf = [0u8; 8];
        buf.copy_from_slice(&seed[i * 8..(i + 1) * 8]);
        let raw = u64::from_le_bytes(buf);
        let v = (u128::from(raw) % GOLDILOCKS_PRIME) as u64;
        *slot = Val::new(v);
    }
    out
}

/// Pad a prefix string into a `[u8; SEED_BYTES]` with `!` filler so the
/// committed seed identity is human-readable without hand-counting bytes.
const fn make_seed(prefix: &[u8]) -> [u8; SEED_BYTES] {
    let mut out = [b'!'; SEED_BYTES];
    let len = if prefix.len() < SEED_BYTES {
        prefix.len()
    } else {
        SEED_BYTES
    };
    let mut i = 0;
    while i < len {
        out[i] = prefix[i];
        i += 1;
    }
    out
}

const ID_SEED: [u8; SEED_BYTES] = make_seed(b"zkmcu-pq-semaphore-v0-id-seed-gl");
const SCOPE_SEED: [u8; SEED_BYTES] = make_seed(b"zkmcu-pq-semaphore-v0-scope-seed-gl");
const MESSAGE_SEED: [u8; SEED_BYTES] = make_seed(b"zkmcu-pq-sem-v0-message-payload-gl");

pub fn run(out_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let dir = out_root.join("pq-semaphore-d10-gl");
    fs::create_dir_all(&dir)?;

    let id = seed_to_digest(&ID_SEED);
    let scope = seed_to_digest(&SCOPE_SEED);
    let signal_hash = seed_to_digest(&MESSAGE_SEED);

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
        "wrote pq-semaphore-d10-gl/proof.bin ({} B) + public.bin ({} B)",
        proof_bytes.len(),
        public_bytes.len(),
    );

    Ok(())
}
