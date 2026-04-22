//! Property-based tests with `proptest`. Complements the targeted unit tests
//! in `adversarial.rs` by throwing randomly-shaped inputs at the parsers and
//! verifier, looking for any execution that panics or any tampering that
//! produces `Ok(true)`.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::equatable_if_let,
    clippy::must_use_candidate,
    // `parse_*` return types are non-Copy; `verify` returns `Copy`. Clippy
    // disagrees with itself about `let _ = ...` vs `drop(...)` depending on
    // Copy-ness. The tests here just want "call this and don't panic".
    let_underscore_drop
)]

use std::fs;
use std::path::PathBuf;

use proptest::prelude::*;
use zkmcu_verifier::{parse_proof, parse_public, parse_vk, verify};

fn load(circuit: &str, name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("zkmcu-vectors")
        .join("data")
        .join(circuit)
        .join(name);
    fs::read(&path).unwrap_or_else(|e| panic!("reading {}: {}", path.display(), e))
}

proptest! {
    /// parse_vk must never panic on any byte sequence up to 4 KB. Return Err
    /// is fine; panic is a DoS. We cap the length at 4 KB to keep test runs
    /// fast — larger inputs would be caught by cargo-fuzz over a longer
    /// window.
    #[test]
    fn parse_vk_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..4096)) {
        drop(parse_vk(&bytes));
    }

    /// Same property for parse_proof. Proof size is fixed (256 B) but we
    /// allow longer inputs because the parser should just look at the first
    /// PROOF_SIZE bytes.
    #[test]
    fn parse_proof_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..1024)) {
        drop(parse_proof(&bytes));
    }

    /// Same property for parse_public. The `count` field is adversary-controlled
    /// and leads us through the checked-arithmetic / with-capacity path that
    /// a naive implementation would OOM on — proptest exercises this heavily.
    #[test]
    fn parse_public_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..1024)) {
        drop(parse_public(&bytes));
    }

    /// End-to-end property: if all three parsers succeed, verify must complete
    /// (no panic, some Result) even against completely random inputs.
    #[test]
    fn verify_never_panics_after_parse(
        vk_bytes in prop::collection::vec(any::<u8>(), 0..4096),
        proof_bytes in prop::collection::vec(any::<u8>(), 0..1024),
        public_bytes in prop::collection::vec(any::<u8>(), 0..1024),
    ) {
        if let (Ok(vk), Ok(proof), Ok(public)) = (
            parse_vk(&vk_bytes),
            parse_proof(&proof_bytes),
            parse_public(&public_bytes),
        ) {
            // `verify` returns `Result<bool, Error>` which is `Copy`, so `drop()`
            // would warn; `let _ = ...` is the idiomatic discard here.
            let _ = verify(&vk, &proof, &public);
        }
    }

    /// Random N-byte XOR mask applied to the proof bytes. Any non-zero mask
    /// must produce either a parse error or `Ok(false)`. Nothing may yield
    /// `Ok(true)` — that would be a proof-forgery primitive.
    #[test]
    fn random_proof_mask_never_accepts(
        mask in prop::collection::vec(any::<u8>(), 256..=256),
    ) {
        if mask.iter().all(|&b| b == 0) {
            // Zero-mask = original proof; trivially verifies.
            return Ok(());
        }

        let vk = parse_vk(&load("square", "vk.bin")).unwrap();
        let public = parse_public(&load("square", "public.bin")).unwrap();
        let mut proof_bytes = load("square", "proof.bin");
        for (b, m) in proof_bytes.iter_mut().zip(mask.iter()) {
            *b ^= m;
        }

        if let Ok(proof) = parse_proof(&proof_bytes) {
            let result = verify(&vk, &proof, &public);
            prop_assert!(
                !matches!(result, Ok(true)),
                "tampered proof accepted: mask={mask:?}"
            );
        }
    }

    /// Random N-byte XOR mask applied to the public bytes (after the 4-byte
    /// count prefix, so we test scalar-content tampering, not count mismatch).
    #[test]
    fn random_public_mask_never_accepts(
        mask in prop::collection::vec(any::<u8>(), 32..=32),
    ) {
        if mask.iter().all(|&b| b == 0) {
            return Ok(());
        }

        let vk = parse_vk(&load("square", "vk.bin")).unwrap();
        let proof = parse_proof(&load("square", "proof.bin")).unwrap();
        let mut public_bytes = load("square", "public.bin");
        // public_bytes[0..4] is count=1, bytes[4..36] is the single Fr. Mask the Fr.
        for (b, m) in public_bytes.iter_mut().skip(4).zip(mask.iter()) {
            *b ^= m;
        }

        if let Ok(public) = parse_public(&public_bytes) {
            let result = verify(&vk, &proof, &public);
            prop_assert!(
                !matches!(result, Ok(true)),
                "tampered public accepted: mask={mask:?}"
            );
        }
    }
}
