//! Host-side Plonky3 prover for the audited Poseidon2-`BabyBear`
//! batched-permutation AIR.
//!
//! Output: `crates/zkmcu-vectors/data/p3-poseidon2-chain-bb/proof.bin`. The
//! AIR has no public inputs, so no `public.bin` is emitted — the verifier
//! reconstructs the AIR from compiled-in audited round constants and
//! consumes only the proof bytes.
//!
//! Determinism. Both Plonky3's `VectorizedPoseidon2Air::generate_vectorized\
//! _trace_rows` (seeded `SmallRng` at line 197 of
//! `vendor/Plonky3/poseidon2-air/src/vectorized.rs`) and `TwoAdicFriPcs`
//! (non-hiding, no prover randomness) are byte-deterministic for a fixed
//! AIR + config. Re-running this generator produces identical bytes; the
//! committed `proof.bin` therefore has a stable content hash across
//! regenerations.
//!
//! The verifier and prover MUST share `make_config` / `build_air` from
//! `zkmcu_verifier_plonky3::poseidon2_chain` — the imports below are how
//! we enforce that single source of truth.

use std::fs;
use std::path::Path;

use p3_uni_stark::prove;
use zkmcu_verifier_plonky3::poseidon2_chain::{
    build_air, encode_proof, make_config, parse_proof, verify_proof, VECTOR_LEN,
};
use zkmcu_verifier_plonky3::MAX_PROOF_SIZE;

/// Number of rows in the AIR trace. `1 << 6` keeps the postcard-encoded
/// proof under [`MAX_PROOF_SIZE`] for the wide vectorised trace at the
/// 28-query FRI parameter set; matches the smoke-test scale used by the
/// verifier crate's `tests/poseidon_chain.rs`.
const NUM_ROWS: usize = 1 << 6;
const NUM_PERMUTATIONS: usize = NUM_ROWS * VECTOR_LEN;

pub fn run(out_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let dir = out_root.join("p3-poseidon2-chain-bb");
    fs::create_dir_all(&dir)?;

    let air = build_air();
    let config = make_config();
    let trace = air.generate_vectorized_trace_rows(NUM_PERMUTATIONS, 1);

    let proof = prove(&config, &air, trace, &[]);
    let proof_bytes = encode_proof(&proof).map_err(|e| format!("encode proof: {e:?}"))?;

    if proof_bytes.len() > MAX_PROOF_SIZE {
        return Err(format!(
            "proof size {} exceeds MAX_PROOF_SIZE {MAX_PROOF_SIZE}",
            proof_bytes.len(),
        )
        .into());
    }

    // Self-verify before bytes hit disk via the same parser the firmware
    // uses, so a wire-format regression here cannot produce committed
    // bytes that pass on-host but fail on-MCU.
    let parsed = parse_proof(&proof_bytes).map_err(|e| format!("self-parse: {e:?}"))?;
    verify_proof(&parsed).map_err(|e| format!("self-verify: {e:?}"))?;

    fs::write(dir.join("proof.bin"), &proof_bytes)?;
    println!(
        "wrote p3-poseidon2-chain-bb/proof.bin {} B (NUM_ROWS={NUM_ROWS}, VECTOR_LEN={VECTOR_LEN})",
        proof_bytes.len(),
    );

    Ok(())
}
