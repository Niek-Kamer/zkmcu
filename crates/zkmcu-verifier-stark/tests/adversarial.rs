//! Adversarial / malformed-input tests for the STARK parser and verifier.
//!
//! Same goal as the BN254 and BLS12-381 adversarial suites: prove that
//! `parse_proof`, `parse_public`, and `verify` never panic on adversarial
//! input, always reject invalid inputs, and never accept a tampered proof.
//!
//! STARK specifics vs the pairing suites:
//!
//! - Proof is variable-size (~30 KB for Fibonacci-1024 at 95-bit security)
//!   rather than fixed 256 B / 512 B, so exhaustive bit-flip is infeasible
//!   (~240 k iterations × ~20 ms verify each = ~80 min). We sample N=256
//!   random bit positions instead, with a fixed seed so a CI failure is
//!   bit-reproducible.
//! - There is no verifying key, the AIR compiled into the firmware *is*
//!   the verifier invariant. Cross-AIR rejection tests play the role that
//!   cross-VK tests do for Groth16.
//! - `verify` takes ownership of `Proof` (not a reference) so every
//!   iteration re-parses the tampered bytes.
//!
//! Skips cleanly if the committed fixtures aren't present locally, same
//! convention as `fibonacci_roundtrip.rs`.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::must_use_candidate,
    clippy::similar_names,
    clippy::print_stderr
)]

use std::fs;
use std::path::PathBuf;

use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;

use zkmcu_verifier_stark::{fibonacci, fibonacci_babybear, parse_proof, Error};

// ---- Fixtures ----------------------------------------------------------

fn stark_dir(slug: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("zkmcu-vectors")
        .join("data")
        .join(slug)
}

fn load_optional(slug: &str, name: &str) -> Option<Vec<u8>> {
    let path = stark_dir(slug).join(name);
    fs::read(&path).ok()
}

fn load_goldilocks() -> Option<(Vec<u8>, Vec<u8>)> {
    let proof = load_optional("stark-fib-1024", "proof.bin")?;
    let public = load_optional("stark-fib-1024", "public.bin")?;
    Some((proof, public))
}

fn load_babybear() -> Option<(Vec<u8>, Vec<u8>)> {
    let proof = load_optional("stark-fib-1024-babybear", "proof.bin")?;
    let public = load_optional("stark-fib-1024-babybear", "public.bin")?;
    Some((proof, public))
}

// ---- parse_proof -------------------------------------------------------

#[test]
fn parse_proof_empty_rejects() {
    assert!(matches!(parse_proof(&[]), Err(Error::ProofDeserialization)));
}

#[test]
fn parse_proof_one_byte_rejects() {
    assert!(matches!(
        parse_proof(&[0u8]),
        Err(Error::ProofDeserialization)
    ));
}

#[test]
fn parse_proof_random_small_buffers_never_panic() {
    // No panic / abort on adversarial short buffers. Winterfell's own
    // deserializer must reject cleanly below its own minimum-size threshold.
    let mut rng = ChaCha20Rng::seed_from_u64(0x57A7_C0DE_D15A_B1ED);
    let mut buf = [0u8; 512];
    for _ in 0..256 {
        let len = (rng.next_u32() as usize) % 512;
        let bytes = &mut buf[..len];
        rng.fill_bytes(bytes);
        // Only assertion: doesn't panic. Err is the expected outcome; Ok on
        // random short bytes would be a surprise but not strictly a bug.
        drop(parse_proof(bytes));
    }
}

#[test]
fn parse_proof_truncated_good_bytes_rejects() {
    let Some((good_proof, _)) = load_goldilocks() else {
        eprintln!("skipping: stark-fib-1024 fixture not present");
        return;
    };
    // Cutting the last byte off the structured proof must always reject.
    // Winterfell's deserializer carries explicit length prefixes for its
    // vectors, so a short tail means an unfinished vector-read.
    let truncated = &good_proof[..good_proof.len() - 1];
    assert!(matches!(
        parse_proof(truncated),
        Err(Error::ProofDeserialization)
    ));
}

// ---- parse_public (goldilocks) ----------------------------------------

#[test]
fn parse_public_empty_rejects() {
    assert!(matches!(
        fibonacci::parse_public(&[]),
        Err(Error::PublicDeserialization)
    ));
}

#[test]
fn parse_public_seven_bytes_rejects() {
    // PUBLIC_SIZE is 8. Anything shorter should reject.
    assert!(matches!(
        fibonacci::parse_public(&[0u8; 7]),
        Err(Error::PublicDeserialization)
    ));
}

#[test]
fn parse_public_any_u64_accepted() {
    // BaseElement::new reduces mod the Goldilocks prime, so every 8-byte
    // input is a valid public-input encoding. Document this by exercising
    // zero, one, and max. If upstream ever tightens to reject ≥ modulus we
    // should catch it here and decide whether to mirror the strictness.
    fibonacci::parse_public(&0u64.to_le_bytes()).expect("zero u64 parses");
    fibonacci::parse_public(&1u64.to_le_bytes()).expect("one u64 parses");
    fibonacci::parse_public(&u64::MAX.to_le_bytes()).expect("u64::MAX parses");
}

#[test]
fn parse_public_rejects_trailing_bytes() {
    // Previous contract accepted `bytes.len() >= PUBLIC_SIZE` and silently
    // dropped the tail, a malleability surface for callers that hash the
    // public-input bytes as a nullifier or replay tag. Strict length check
    // closed that. Pin the new behaviour so any future loosening is an
    // explicit decision, not a silent regression.
    let base: [u8; 12] = [1, 0, 0, 0, 0, 0, 0, 0, 0xFF, 0xEE, 0xDD, 0xCC];
    assert!(matches!(
        fibonacci::parse_public(&base),
        Err(Error::PublicDeserialization)
    ));
    // And the exact-length slice still parses fine.
    fibonacci::parse_public(&base[..8]).expect("exact PUBLIC_SIZE parses");
}

#[test]
fn parse_public_babybear_rejects_trailing_bytes() {
    let base: [u8; 8] = [1, 0, 0, 0, 0xFF, 0xEE, 0xDD, 0xCC];
    assert!(matches!(
        fibonacci_babybear::parse_public(&base),
        Err(Error::PublicDeserialization)
    ));
    fibonacci_babybear::parse_public(&base[..4]).expect("exact PUBLIC_SIZE parses");
}

#[test]
fn parse_proof_rejects_trailing_bytes() {
    // Take a known-good proof, append one byte, assert rejection. Winter-
    // fell's `Proof::from_bytes` alone would accept this because the
    // underlying `Deserializable::read_from_bytes` tolerates trailing
    // bytes; the wrapper enforces strictness via `SliceReader::\
    // has_more_bytes` after `read_from`.
    let Some((proof_bytes, _)) = load_goldilocks() else {
        eprintln!("skipping: stark-fib-1024 fixture not present");
        return;
    };
    let mut padded = proof_bytes.clone();
    padded.push(0x00);
    assert!(matches!(
        parse_proof(&padded),
        Err(Error::ProofDeserialization)
    ));
    // And the exact-length slice still parses + verifies.
    let proof = parse_proof(&proof_bytes).expect("clean proof parses");
    drop(proof);
}

// ---- verify (Fibonacci, Goldilocks) -----------------------------------

#[test]
fn verify_known_good_fixture_accepts() {
    // Sanity: this is essentially fibonacci_roundtrip.rs, but inside the
    // adversarial file too so a regression that lets the known-good proof
    // fail verify is flagged alongside the adversarial tests.
    let Some((proof_bytes, public_bytes)) = load_goldilocks() else {
        eprintln!("skipping: stark-fib-1024 fixture not present");
        return;
    };
    let proof = parse_proof(&proof_bytes).expect("parse proof");
    let public = fibonacci::parse_public(&public_bytes).expect("parse public");
    fibonacci::verify(proof, public).expect("known-good proof must verify");
}

#[test]
fn verify_rejects_wrong_public() {
    // Keep the proof; change public-inputs to a value the proof does not
    // bind to. The `s_{1, last} = result` assertion inside the Fibonacci
    // AIR must drive a constraint-violation rejection.
    let Some((proof_bytes, public_bytes)) = load_goldilocks() else {
        eprintln!("skipping: stark-fib-1024 fixture not present");
        return;
    };
    let proof = parse_proof(&proof_bytes).expect("parse proof");

    // Flip the low bit of the result: turns Fib(2048) into Fib(2048)+1 mod p.
    let mut wrong_public = public_bytes;
    wrong_public[0] ^= 1;
    let wrong = fibonacci::parse_public(&wrong_public).expect("parse wrong public");

    let result = fibonacci::verify(proof, wrong);
    assert!(
        matches!(result, Err(Error::Verification(_))),
        "wrong public must surface as Verification error, got {result:?}"
    );
}

#[test]
fn verify_rejects_random_proof_bitflips() {
    // N=256 sampled bit positions. For each: flip, re-parse, re-verify,
    // assert the tampered proof never accepts. Parse failures are fine
    // (most flips break deserialization cheaply), the only bad outcome is
    // `Ok(())` with mutated bytes.
    let Some((proof_bytes, public_bytes)) = load_goldilocks() else {
        eprintln!("skipping: stark-fib-1024 fixture not present");
        return;
    };
    let public = fibonacci::parse_public(&public_bytes).expect("parse public");

    let mut rng = ChaCha20Rng::seed_from_u64(0x5AD_D15C_0DE1_B15E);
    let mut accepted_despite_tamper: usize = 0;
    let mut parse_failures: usize = 0;
    let mut verify_failures: usize = 0;

    for _ in 0..256 {
        let byte_idx = (rng.next_u32() as usize) % proof_bytes.len();
        let bit_idx = u8::try_from(rng.next_u32() & 7).expect("mask & 7 is in 0..8");
        let mut tampered = proof_bytes.clone();
        tampered[byte_idx] ^= 1u8 << bit_idx;

        match parse_proof(&tampered) {
            Err(_) => parse_failures += 1,
            Ok(proof) => match fibonacci::verify(proof, public) {
                Ok(()) => accepted_despite_tamper += 1,
                Err(_) => verify_failures += 1,
            },
        }
    }

    assert_eq!(
        accepted_despite_tamper, 0,
        "{accepted_despite_tamper} sampled bit-flip proof mutations produced Ok(()), \
         this is a soundness bug. parse_failures={parse_failures}, \
         verify_failures={verify_failures}"
    );
}

#[test]
fn verify_rejects_random_public_bitflips() {
    // Public inputs are small (8 bytes = 64 bits) so we can do the full
    // exhaustive single-bit sweep cheaply, 64 × ~20 ms ≈ 1.3 s.
    let Some((proof_bytes, public_bytes)) = load_goldilocks() else {
        eprintln!("skipping: stark-fib-1024 fixture not present");
        return;
    };

    let mut accepted = 0usize;

    for byte_idx in 0..public_bytes.len() {
        for bit in 0..8 {
            let mut tampered = public_bytes.clone();
            tampered[byte_idx] ^= 1u8 << bit;
            let Ok(public) = fibonacci::parse_public(&tampered) else {
                continue;
            };
            // Re-parse the (clean) proof per iteration, verify consumes it.
            let proof = parse_proof(&proof_bytes).expect("clean proof reparses");
            if fibonacci::verify(proof, public).is_ok() {
                accepted += 1;
            }
        }
    }

    assert_eq!(
        accepted, 0,
        "{accepted} public-input single-bit mutations still verified, soundness bug"
    );
}

// ---- Cross-AIR rejection ----------------------------------------------

#[test]
fn goldilocks_verifier_rejects_babybear_proof() {
    // Feed a proof generated for the BabyBear AIR into the Goldilocks
    // Fibonacci verifier. Either `Proof::from_bytes` rejects it
    // (field-metadata mismatch baked into the winterfell encoding) or
    // parse succeeds and `verify` rejects. Either is correct; the only
    // outcome we forbid is `Ok(())`.
    let Some((bb_proof_bytes, _)) = load_babybear() else {
        eprintln!("skipping: stark-fib-1024-babybear fixture not present");
        return;
    };
    let Some((_, gl_public_bytes)) = load_goldilocks() else {
        eprintln!("skipping: stark-fib-1024 fixture not present");
        return;
    };
    let public = fibonacci::parse_public(&gl_public_bytes).expect("parse goldilocks public");

    let accepted = parse_proof(&bb_proof_bytes)
        .ok()
        .is_some_and(|proof| fibonacci::verify(proof, public).is_ok());
    assert!(
        !accepted,
        "goldilocks verifier accepted a babybear proof, cross-AIR soundness hole"
    );
}

// ---- Fuzz-found regression fixtures -----------------------------------
//
// `cargo fuzz run stark_parse_proof` on 2026-04-24 found inputs that tripped
// an unbounded `Vec::with_capacity` inside `Queries::read_from`, an adversary-
// controlled length prefix drove winterfell into a ~300 TB allocation and
// aborted the process via `handle_alloc_error`. Patched in the `vendor/\
// winterfell` fork by bounding `read_many`'s `with_capacity` by the source
// reader's `remaining_bytes()` (trait method added for this). These two
// artifacts must now return `Err(ProofDeserialization)` cleanly.
//
// Background: `research/postmortems/2026-04-24-stark-unbounded-vec-alloc.typ`.

fn fuzz_regression_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
        .join("fuzz-regressions")
        .join(name)
}

#[test]
fn fuzz_regression_unbounded_vec_alloc_a() {
    let bytes = fs::read(fuzz_regression_path("unbounded_vec_alloc_a.bin"))
        .expect("fuzz regression fixture present");
    assert!(
        matches!(parse_proof(&bytes), Err(Error::ProofDeserialization)),
        "fuzz regression A: parse_proof must return ProofDeserialization once the fix lands"
    );
}

#[test]
fn fuzz_regression_unbounded_vec_alloc_b() {
    let bytes = fs::read(fuzz_regression_path("unbounded_vec_alloc_b.bin"))
        .expect("fuzz regression fixture present");
    assert!(
        matches!(parse_proof(&bytes), Err(Error::ProofDeserialization)),
        "fuzz regression B: parse_proof must return ProofDeserialization once the fix lands"
    );
}

#[test]
fn parse_proof_rejects_input_above_max_size() {
    // MAX_PROOF_SIZE cap must fire before winterfell's deserializer runs.
    // Build a buffer that would pass the first-4-bytes sanity check but is
    // one byte over the cap, `Err(ProofDeserialization)` is the required
    // outcome (not a panic from winterfell attempting to read 128 KB+1).
    let mut bytes = vec![0u8; zkmcu_verifier_stark::MAX_PROOF_SIZE + 1];
    // First 4 bytes: main=1, aux=0, rands=0, log2_len=3, passes header check.
    bytes[0] = 1;
    bytes[3] = 3;
    assert!(matches!(
        parse_proof(&bytes),
        Err(Error::ProofDeserialization)
    ));
}

#[test]
fn babybear_verifier_rejects_goldilocks_proof() {
    // Symmetric to `goldilocks_verifier_rejects_babybear_proof`. Historically
    // this direction panicked inside winterfell's `from_bytes_with_padding`
    // when BabyBear's 4-byte elements met the goldilocks proof's 8-byte
    // chunks (Path 1 in the finding). The wrapper fix in
    // `fibonacci_babybear::verify` now rejects cross-field proofs via an
    // embedded-modulus check before any byte conversion runs, so we expect
    // a clean `Err` here instead of a panic, no catch_unwind.
    let Some((gl_proof_bytes, _)) = load_goldilocks() else {
        eprintln!("skipping: stark-fib-1024 fixture not present");
        return;
    };
    let Some((_, bb_public_bytes)) = load_babybear() else {
        eprintln!("skipping: stark-fib-1024-babybear fixture not present");
        return;
    };
    let public = fibonacci_babybear::parse_public(&bb_public_bytes).expect("parse babybear public");

    let accepted = parse_proof(&gl_proof_bytes)
        .ok()
        .is_some_and(|proof| fibonacci_babybear::verify(proof, public).is_ok());
    assert!(
        !accepted,
        "babybear verifier accepted a goldilocks proof, cross-AIR soundness hole"
    );
}
