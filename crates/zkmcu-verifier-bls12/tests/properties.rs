//! Property-based tests with `proptest`. Parity with
//! `zkmcu-verifier/tests/properties.rs`, adapted for BLS12-381 wire sizes.
//!
//! The goal is identical: throw random-shaped inputs at the parsers and
//! verifier, look for any execution that panics or any tampering that
//! produces `Ok(true)`.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::equatable_if_let,
    clippy::must_use_candidate,
    let_underscore_drop
)]

use std::fs;
use std::path::PathBuf;

use proptest::prelude::*;
use zkmcu_verifier_bls12::{parse_proof, parse_public, parse_vk, verify, PROOF_SIZE};

fn load(circuit: &str, name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("zkmcu-vectors")
        .join("data")
        .join("bls12-381")
        .join(circuit)
        .join(name);
    fs::read(&path).unwrap_or_else(|e| panic!("reading {}: {}", path.display(), e))
}

proptest! {
    /// parse_vk must never panic on any byte sequence up to 4 KB. Larger
    /// inputs are fuzzing-territory, not proptest-territory; cap kept short
    /// so test runs stay fast.
    #[test]
    fn parse_vk_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..4096)) {
        drop(parse_vk(&bytes));
    }

    /// Same property for parse_proof. Proof size is fixed at PROOF_SIZE (512 B)
    /// but longer inputs are allowed, the parser should just look at the
    /// first PROOF_SIZE bytes.
    #[test]
    fn parse_proof_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..1024)) {
        drop(parse_proof(&bytes));
    }

    /// parse_public's `count` field is adversary-controlled and drives the
    /// checked-arithmetic / Vec::with_capacity path that a naive parser
    /// would OOM on. Proptest exercises this heavily.
    #[test]
    fn parse_public_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..1024)) {
        drop(parse_public(&bytes));
    }

    /// End-to-end property: if all three parsers succeed, verify must
    /// complete (no panic, some Result) even against completely random
    /// inputs.
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
            let _ = verify(&vk, &proof, &public);
        }
    }

    /// Random PROOF_SIZE-byte XOR mask applied to the proof bytes. Any
    /// non-zero mask must produce either a parse error or `Ok(false)`.
    /// Nothing may yield `Ok(true)`, that would be a proof-forgery primitive.
    #[test]
    fn random_proof_mask_never_accepts(
        mask in prop::collection::vec(any::<u8>(), PROOF_SIZE..=PROOF_SIZE),
    ) {
        if mask.iter().all(|&b| b == 0) {
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

    /// Random 32-byte XOR mask applied to the public bytes (after the 4-byte
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
