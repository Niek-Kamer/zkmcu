//! Host-side Plonky3 prover for the PQ-Semaphore dual-hash bench.
//!
//! Phase E.1 of the 128-bit security plan: build the same Phase B witness
//! (BabyBear × Quartic, DIGEST_WIDTH=6, FRI grinding 16+16) and prove it
//! TWICE — once under the audited Poseidon2-`BabyBear` config and once
//! under a Blake3 commitment. Outputs:
//!
//! - `crates/zkmcu-vectors/data/pq-semaphore-d10-dual/proof_p2.bin`
//! - `crates/zkmcu-vectors/data/pq-semaphore-d10-dual/proof_b3.bin`
//! - `crates/zkmcu-vectors/data/pq-semaphore-d10-dual/public.bin`
//!
//! Both proofs commit to the same trace, the same public inputs, and the
//! same (id, scope, signal_hash) triple. The dual verifier accepts iff
//! both proofs verify; this composes soundness across two cryptographically
//! independent hashes (Poseidon2 algebraic vs Blake3 generic).
//!
//! ## Why a separate trace per prove call?
//!
//! Plonky3's `prove(...)` consumes the trace `RowMajorMatrix` by value.
//! We re-build the trace twice (deterministic, ~16 rows × ~332 cols of
//! BabyBear) — cost is negligible compared to the FRI work either prover
//! does. Witness construction is shared and only runs once.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::indexing_slicing)]
#![allow(clippy::similar_names)]
// Telemetry only: prove/verify wallclock printed to stdout for the writeup
// host-side numbers. Float arithmetic stays out of any verifier code path.
#![allow(clippy::float_arithmetic)]

use std::fs;
use std::path::Path;
use std::time::Instant;

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use p3_matrix::dense::RowMajorMatrix;
use p3_uni_stark::prove;
use zkmcu_verifier_plonky3::pq_semaphore::{
    build_air, build_trace_values, build_witness, encode_proof, encode_public_inputs, make_config,
    pack_public_inputs, parse_proof, parse_public_inputs, trace_width, verify_with_config,
    DIGEST_WIDTH,
};
use zkmcu_verifier_plonky3::pq_semaphore_blake3::{
    encode_proof as encode_proof_b3, make_config as make_config_b3, parse_proof as parse_proof_b3,
    verify_with_config as verify_with_config_b3,
};
use zkmcu_verifier_plonky3::MAX_PROOF_SIZE;

type Val = BabyBear;

/// Length in bytes required to derive `DIGEST_WIDTH` `BabyBear` elements
/// (8 bytes per element, splitmix-style).
const SEED_BYTES: usize = DIGEST_WIDTH * 8;

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

const ID_SEED: [u8; SEED_BYTES] = make_seed(b"zkmcu-pq-semaphore-v0-id-seed-dual");
const SCOPE_SEED: [u8; SEED_BYTES] = make_seed(b"zkmcu-pq-semaphore-v0-scope-seed-dual");
const MESSAGE_SEED: [u8; SEED_BYTES] = make_seed(b"zkmcu-pq-sem-v0-message-payload-dual");

pub fn run(out_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let dir = out_root.join("pq-semaphore-d10-dual");
    fs::create_dir_all(&dir)?;

    let id = seed_to_digest(&ID_SEED);
    let scope = seed_to_digest(&SCOPE_SEED);
    let signal_hash = seed_to_digest(&MESSAGE_SEED);

    let witness = build_witness(id, scope, signal_hash);
    let public = pack_public_inputs(&witness);
    let public_bytes = encode_public_inputs(&public);
    let parsed_public =
        parse_public_inputs(&public_bytes).map_err(|e| format!("self-parse public: {e:?}"))?;
    debug_assert_eq!(parsed_public, public);

    // ---- Poseidon2 leg ----
    let trace_p2 = RowMajorMatrix::new(build_trace_values(&witness), trace_width());
    let air = build_air();
    let config_p2 = make_config();
    let t_prove_p2 = Instant::now();
    let proof_p2 = prove(&config_p2, &air, trace_p2, &public[..]);
    let prove_p2_ms = t_prove_p2.elapsed().as_secs_f64() * 1000.0;
    println!("[bench] prove(p2) = {prove_p2_ms:.2} ms");
    let proof_p2_bytes = encode_proof(&proof_p2).map_err(|e| format!("encode p2: {e:?}"))?;
    if proof_p2_bytes.len() > MAX_PROOF_SIZE {
        return Err(format!(
            "proof_p2 size {} exceeds MAX_PROOF_SIZE {MAX_PROOF_SIZE}",
            proof_p2_bytes.len(),
        )
        .into());
    }
    let parsed_p2 = parse_proof(&proof_p2_bytes).map_err(|e| format!("self-parse p2: {e:?}"))?;
    let t_verify_p2 = Instant::now();
    verify_with_config(&parsed_p2, &parsed_public, &config_p2, &air)
        .map_err(|e| format!("self-verify p2: {e:?}"))?;
    let verify_p2_ms = t_verify_p2.elapsed().as_secs_f64() * 1000.0;
    println!("[bench] verify(p2) host = {verify_p2_ms:.2} ms");

    // ---- Blake3 leg ----
    let trace_b3 = RowMajorMatrix::new(build_trace_values(&witness), trace_width());
    let config_b3 = make_config_b3();
    let t_prove_b3 = Instant::now();
    let proof_b3 = prove(&config_b3, &air, trace_b3, &public[..]);
    let prove_b3_ms = t_prove_b3.elapsed().as_secs_f64() * 1000.0;
    println!("[bench] prove(b3) = {prove_b3_ms:.2} ms");
    let proof_b3_bytes = encode_proof_b3(&proof_b3).map_err(|e| format!("encode b3: {e:?}"))?;
    if proof_b3_bytes.len() > MAX_PROOF_SIZE {
        return Err(format!(
            "proof_b3 size {} exceeds MAX_PROOF_SIZE {MAX_PROOF_SIZE}",
            proof_b3_bytes.len(),
        )
        .into());
    }
    let parsed_b3 = parse_proof_b3(&proof_b3_bytes).map_err(|e| format!("self-parse b3: {e:?}"))?;
    let t_verify_b3 = Instant::now();
    verify_with_config_b3(&parsed_b3, &parsed_public, &config_b3, &air)
        .map_err(|e| format!("self-verify b3: {e:?}"))?;
    let verify_b3_ms = t_verify_b3.elapsed().as_secs_f64() * 1000.0;
    println!("[bench] verify(b3) host = {verify_b3_ms:.2} ms");
    println!(
        "[bench] dual prove total = {:.2} ms (p2 {prove_p2_ms:.2} + b3 {prove_b3_ms:.2})",
        prove_p2_ms + prove_b3_ms,
    );
    println!(
        "[bench] dual verify total host = {:.2} ms (p2 {verify_p2_ms:.2} + b3 {verify_b3_ms:.2})",
        verify_p2_ms + verify_b3_ms,
    );

    fs::write(dir.join("proof_p2.bin"), &proof_p2_bytes)?;
    fs::write(dir.join("proof_b3.bin"), &proof_b3_bytes)?;
    fs::write(dir.join("public.bin"), &public_bytes)?;

    println!(
        "wrote pq-semaphore-d10-dual/proof_p2.bin ({} B) + proof_b3.bin ({} B) + public.bin ({} B)",
        proof_p2_bytes.len(),
        proof_b3_bytes.len(),
        public_bytes.len(),
    );

    Ok(())
}
