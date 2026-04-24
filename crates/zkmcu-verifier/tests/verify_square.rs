//! End-to-end cross-check: parse the EIP-197 bytes produced by zkmcu-host-gen
//! (which uses arkworks) and verify them with zkmcu-verifier (which uses substrate-bn).
//! A passing run proves the two crypto stacks agree on encoding and group operations.

// Tests are expected to panic loudly on failure; these lints are the wrong shape here.
#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]

use std::fs;
use std::path::PathBuf;

fn load_from(circuit: &str, name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("zkmcu-vectors")
        .join("data")
        .join(circuit)
        .join(name);
    fs::read(&path).unwrap_or_else(|e| panic!("reading {}: {}", path.display(), e))
}

fn load(name: &str) -> Vec<u8> {
    load_from("square", name)
}

#[test]
fn square_vector_verifies() {
    let vk = zkmcu_verifier::parse_vk(&load("vk.bin")).expect("parse vk");
    let proof = zkmcu_verifier::parse_proof(&load("proof.bin")).expect("parse proof");
    let public = zkmcu_verifier::parse_public(&load("public.bin")).expect("parse public");

    let ok = zkmcu_verifier::verify(&vk, &proof, &public).expect("verify");
    assert!(ok, "Groth16 verify returned false on known-good vector");
}

#[test]
fn square_vector_rejects_tampered_public() {
    let vk = zkmcu_verifier::parse_vk(&load("vk.bin")).expect("parse vk");
    let proof = zkmcu_verifier::parse_proof(&load("proof.bin")).expect("parse proof");
    let mut public_bytes = load("public.bin");

    // Flip a bit in the public input, verifier must reject.
    let last = public_bytes.len() - 1;
    public_bytes[last] ^= 0x01;
    let public = zkmcu_verifier::parse_public(&public_bytes).expect("parse public");

    let ok = zkmcu_verifier::verify(&vk, &proof, &public).expect("verify");
    assert!(!ok, "verifier accepted tampered public input");
}

#[test]
fn squares_5_vector_verifies() {
    let vk = zkmcu_verifier::parse_vk(&load_from("squares-5", "vk.bin")).expect("parse vk");
    let proof =
        zkmcu_verifier::parse_proof(&load_from("squares-5", "proof.bin")).expect("parse proof");
    let public =
        zkmcu_verifier::parse_public(&load_from("squares-5", "public.bin")).expect("parse public");

    assert_eq!(vk.ic.len(), 6, "5 public inputs → 6 IC points");
    assert_eq!(public.len(), 5);

    let ok = zkmcu_verifier::verify(&vk, &proof, &public).expect("verify");
    assert!(
        ok,
        "Groth16 verify returned false on known-good squares-5 vector"
    );
}

#[test]
fn squares_5_rejects_tampered_last_input() {
    let vk = zkmcu_verifier::parse_vk(&load_from("squares-5", "vk.bin")).expect("parse vk");
    let proof =
        zkmcu_verifier::parse_proof(&load_from("squares-5", "proof.bin")).expect("parse proof");
    let mut public_bytes = load_from("squares-5", "public.bin");

    // Flip a bit in the last public input, verifier must reject.
    let last = public_bytes.len() - 1;
    public_bytes[last] ^= 0x01;
    let public = zkmcu_verifier::parse_public(&public_bytes).expect("parse public");

    let ok = zkmcu_verifier::verify(&vk, &proof, &public).expect("verify");
    assert!(!ok, "verifier accepted tampered squares-5 input");
}
