//! Host-side Plonky3 prover for the PQ-Semaphore AIR.
//!
//! Output:
//! - `crates/zkmcu-vectors/data/pq-semaphore-d10/proof.bin`
//! - `crates/zkmcu-vectors/data/pq-semaphore-d10/public.bin`
//!
//! ## Witness construction
//!
//! Deterministic seeds, each `DIGEST_WIDTH * 8 = 48` bytes wide:
//! - `id` derived from `b"zkmcu-pq-semaphore-v0-id-seed-d6!!!!!!!!!!!!!!!!"`
//! - `scope` derived from `b"zkmcu-pq-semaphore-v0-scope-seed-d6!!!!!!!!!!!"`
//! - `message` derived from `b"zkmcu-pq-sem-v0-message-payload-d6!!!!!!!!!!!!"`
//!
//! Each 48-byte seed is squeezed into 6 `BabyBear` elements via
//! splitmix-style chunking. `signal_hash` is `message` directly (we
//! treat the message as already pre-hashed for v0; downstream callers
//! can swap in a real hasher if needed). Merkle tree: depth 10, 1024
//! leaves. Leaf 0 is `H(id || 0^10)`, leaves 1..1024 are
//! `H(i || 0^10)` for `i = 1..1023`. The prover claims membership of
//! leaf 0; the path is trivially "all left" (direction bits all zero)
//! and siblings are computed bottom-up.
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

/// Length in bytes required to derive `DIGEST_WIDTH` `BabyBear` elements
/// (8 bytes per element, splitmix-style).
const SEED_BYTES: usize = DIGEST_WIDTH * 8;

/// Squeeze an 8 × `DIGEST_WIDTH`-byte seed into `DIGEST_WIDTH` canonical
/// `BabyBear` elements.
fn seed_to_digest(seed: &[u8; SEED_BYTES]) -> [Val; DIGEST_WIDTH] {
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

const ID_SEED: [u8; SEED_BYTES] = make_seed(b"zkmcu-pq-semaphore-v0-id-seed-d6");
const SCOPE_SEED: [u8; SEED_BYTES] = make_seed(b"zkmcu-pq-semaphore-v0-scope-seed-d6");
const MESSAGE_SEED: [u8; SEED_BYTES] = make_seed(b"zkmcu-pq-sem-v0-message-payload-d6");

pub fn run(out_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let dir = out_root.join("pq-semaphore-d10");
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
        "wrote pq-semaphore-d10/proof.bin ({} B) + public.bin ({} B)",
        proof_bytes.len(),
        public_bytes.len(),
    );

    Ok(())
}
