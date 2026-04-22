//! Adversarial / malformed-input tests for the parsers and `verify`.
//!
//! Goal of this file: prove that `parse_vk`, `parse_proof`, `parse_public`,
//! and `verify` never panic on adversarial input, always reject invalid
//! inputs, and never accept a proof that has been tampered with. A bug found
//! here would be a real security finding; an unexpected `Ok(true)` with
//! modified bytes would be exploitable.

// Tests are allowed to panic loudly on unexpected behaviour and use ergonomic
// patterns that clippy pedantic mode flags.
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::equatable_if_let,
    clippy::similar_names,
    clippy::must_use_candidate
)]

use std::fs;
use std::path::PathBuf;

use zkmcu_verifier::{
    parse_proof, parse_public, parse_vk, verify, Error, FR_SIZE, G1_SIZE, G2_SIZE, PROOF_SIZE,
};

// ---- Helpers -----------------------------------------------------------

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

fn known_good() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    (
        load_from("square", "vk.bin"),
        load_from("square", "proof.bin"),
        load_from("square", "public.bin"),
    )
}

// ---- parse_vk ----------------------------------------------------------

#[test]
fn parse_vk_empty() {
    assert!(matches!(parse_vk(&[]), Err(Error::TruncatedInput)));
}

#[test]
fn parse_vk_one_byte_short() {
    let (vk, _, _) = known_good();
    let truncated = &vk[..vk.len() - 1];
    assert!(matches!(parse_vk(truncated), Err(Error::TruncatedInput)));
}

#[test]
fn parse_vk_truncated_before_ic() {
    // Just the header (alpha + beta + gamma + delta), no ic-count field.
    let header = G1_SIZE + 3 * G2_SIZE;
    let (vk, _, _) = known_good();
    let truncated = &vk[..header];
    assert!(matches!(parse_vk(truncated), Err(Error::TruncatedInput)));
}

#[test]
fn parse_vk_claimed_ic_count_overflows() {
    // Header valid, but num_ic = u32::MAX. ic_start + num_ic * G1_SIZE must not
    // panic / overflow — it must detect insufficient buffer and return Truncated.
    let (mut vk, _, _) = known_good();
    let header = G1_SIZE + 3 * G2_SIZE;
    vk[header..header + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(parse_vk(&vk), Err(Error::TruncatedInput)));
}

#[test]
fn parse_vk_zero_ic_count() {
    // num_ic = 0 is structurally valid; parse succeeds with an empty ic vec.
    let (mut vk, _, _) = known_good();
    let header = G1_SIZE + 3 * G2_SIZE;
    vk[header..header + 4].copy_from_slice(&0u32.to_le_bytes());
    // Truncate to just the header + count, since no IC entries follow.
    let truncated: Vec<u8> = vk[..header + 4].to_vec();
    let parsed = parse_vk(&truncated).expect("parse zero-ic vk");
    assert_eq!(parsed.ic.len(), 0);
}

#[test]
fn parse_vk_field_element_above_modulus() {
    // The first 32 bytes are the x-coordinate of alpha (G1). BN254 base field
    // modulus is < 2^254. Setting the top bits to 1 pushes the value above it.
    let (mut vk, _, _) = known_good();
    vk[0] = 0xff;
    vk[1] = 0xff;
    vk[2] = 0xff;
    vk[3] = 0xff;
    // With the top bits set, the x should exceed the modulus → Fq::from_slice rejects.
    assert!(matches!(
        parse_vk(&vk),
        Err(Error::InvalidFq | Error::InvalidG1)
    ));
}

#[test]
fn parse_vk_all_zero_header_and_no_ic() {
    // All-zero G1 point is the canonical identity in EIP-197. All-zero G2 is
    // the identity on the twist. These should parse successfully as identity
    // points; the verify step catches that such a VK is not valid against any
    // meaningful proof.
    let header = G1_SIZE + 3 * G2_SIZE;
    let mut bytes = vec![0u8; header + 4];
    // num_ic = 0 (already zero)
    let parsed = parse_vk(&bytes).expect("all-zero vk parses");
    assert_eq!(parsed.ic.len(), 0);

    // And a trivially-wrong claim of 1 ic entry with no bytes for it → truncated.
    bytes[header..header + 4].copy_from_slice(&1u32.to_le_bytes());
    assert!(matches!(parse_vk(&bytes), Err(Error::TruncatedInput)));
}

// ---- parse_proof -------------------------------------------------------

#[test]
fn parse_proof_empty() {
    assert!(matches!(parse_proof(&[]), Err(Error::TruncatedInput)));
}

#[test]
fn parse_proof_one_byte_short() {
    let (_, proof, _) = known_good();
    let truncated = &proof[..PROOF_SIZE - 1];
    assert!(matches!(parse_proof(truncated), Err(Error::TruncatedInput)));
}

#[test]
fn parse_proof_field_element_above_modulus() {
    let (_, mut proof, _) = known_good();
    for b in &mut proof[..4] {
        *b = 0xff;
    }
    assert!(matches!(
        parse_proof(&proof),
        Err(Error::InvalidFq | Error::InvalidG1)
    ));
}

#[test]
fn parse_proof_point_not_on_curve() {
    // Build a proof with a known-invalid G1 point at A: (x=1, y=1). On the BN254
    // curve y^2 = x^3 + 3, so (1,1) → 1 != 1 + 3 = 4, not on curve.
    let mut proof = vec![0u8; PROOF_SIZE];
    proof[31] = 1; // A.x = 1
    proof[63] = 1; // A.y = 1
                   // B can stay zero (identity) and C zero, doesn't matter — parser rejects A first.
    assert!(matches!(parse_proof(&proof), Err(Error::InvalidG1)));
}

#[test]
fn parse_proof_all_zero_is_identity_points() {
    // All zeros → A=B=C=identity. Parser must accept (EIP-197 convention)
    // but verify MUST NOT return ok=true against a real VK.
    let proof = vec![0u8; PROOF_SIZE];
    let parsed = parse_proof(&proof).expect("all-zero proof parses as identity");
    let (vk_bytes, _, pub_bytes) = known_good();
    let vk = parse_vk(&vk_bytes).unwrap();
    let public = parse_public(&pub_bytes).unwrap();
    let result = verify(&vk, &parsed, &public).expect("verify runs");
    assert!(!result, "identity-points proof accepted against real vk");
}

// ---- parse_public ------------------------------------------------------

#[test]
fn parse_public_empty() {
    assert!(matches!(parse_public(&[]), Err(Error::TruncatedInput)));
}

#[test]
fn parse_public_count_only_no_elements() {
    let bytes = 0u32.to_le_bytes().to_vec();
    let parsed = parse_public(&bytes).expect("count=0 parses");
    assert_eq!(parsed.len(), 0);
}

#[test]
fn parse_public_count_exceeds_available_bytes() {
    // Claim 5 elements, provide 1.
    let mut bytes = 5u32.to_le_bytes().to_vec();
    bytes.extend_from_slice(&[0u8; FR_SIZE]);
    assert!(matches!(parse_public(&bytes), Err(Error::TruncatedInput)));
}

#[test]
fn parse_public_scalar_above_modulus() {
    // Claim 1 element, encode it as 32 bytes of 0xff — exceeds scalar modulus.
    let mut bytes = 1u32.to_le_bytes().to_vec();
    bytes.extend_from_slice(&[0xffu8; FR_SIZE]);
    assert!(matches!(parse_public(&bytes), Err(Error::InvalidFr)));
}

#[test]
fn parse_public_count_astronomical() {
    // Claim 2^31 elements, provide 32 bytes. Must not allocate a gigantic Vec
    // or panic — must return TruncatedInput cleanly.
    let mut bytes = 0x8000_0000u32.to_le_bytes().to_vec();
    bytes.extend_from_slice(&[0u8; FR_SIZE]);
    assert!(matches!(parse_public(&bytes), Err(Error::TruncatedInput)));
}

// ---- verify ------------------------------------------------------------

#[test]
fn verify_rejects_mismatched_public_count() {
    let (vk_b, proof_b, _) = known_good();
    let vk = parse_vk(&vk_b).unwrap();
    let proof = parse_proof(&proof_b).unwrap();

    // square vector has 1 public input; feed it 0.
    let public: Vec<zkmcu_verifier::Fr> = Vec::new();
    assert!(matches!(
        verify(&vk, &proof, &public),
        Err(Error::PublicInputCount)
    ));
}

#[test]
fn verify_rejects_bitflip_in_every_vk_byte() {
    // Exhaustively flip every byte in the VK and confirm verify never returns Ok(true).
    // We either get a parse error (most bytes — any tamper to curve points breaks them)
    // or Ok(false) (for bytes that still parse but yield a different VK).
    let (vk_b, proof_b, public_b) = known_good();
    let proof = parse_proof(&proof_b).unwrap();
    let public = parse_public(&public_b).unwrap();

    let mut accepted = 0usize;
    let mut parse_errors = 0usize;
    let mut verify_false = 0usize;

    for i in 0..vk_b.len() {
        for bit in 0..8 {
            let mut tampered = vk_b.clone();
            tampered[i] ^= 1 << bit;
            match parse_vk(&tampered) {
                Err(_) => parse_errors += 1,
                Ok(vk) => match verify(&vk, &proof, &public) {
                    Ok(true) => accepted += 1,
                    Ok(false) => verify_false += 1,
                    Err(_) => parse_errors += 1,
                },
            }
        }
    }

    assert_eq!(
        accepted, 0,
        "{accepted} single-bit-flip VK mutations produced Ok(true) — this is a security bug. \
         parse_errors={parse_errors}, verify_false={verify_false}"
    );
}

#[test]
fn verify_rejects_bitflip_in_every_proof_byte() {
    let (vk_b, proof_b, public_b) = known_good();
    let vk = parse_vk(&vk_b).unwrap();
    let public = parse_public(&public_b).unwrap();

    let mut accepted = 0usize;

    for i in 0..proof_b.len() {
        for bit in 0..8 {
            let mut tampered = proof_b.clone();
            tampered[i] ^= 1 << bit;
            if let Ok(proof) = parse_proof(&tampered) {
                if let Ok(true) = verify(&vk, &proof, &public) {
                    accepted += 1;
                }
            }
        }
    }

    assert_eq!(
        accepted, 0,
        "{accepted} single-bit-flip proof mutations produced Ok(true) — security bug"
    );
}

#[test]
fn verify_rejects_bitflip_in_every_public_byte() {
    let (vk_b, proof_b, public_b) = known_good();
    let vk = parse_vk(&vk_b).unwrap();
    let proof = parse_proof(&proof_b).unwrap();

    let mut accepted = 0usize;

    // Start from byte 4 to skip the count prefix — changing it just produces
    // a mismatched-count error, not a tampered-scalar attack.
    for i in 4..public_b.len() {
        for bit in 0..8 {
            let mut tampered = public_b.clone();
            tampered[i] ^= 1 << bit;
            if let Ok(public) = parse_public(&tampered) {
                if let Ok(true) = verify(&vk, &proof, &public) {
                    accepted += 1;
                }
            }
        }
    }

    assert_eq!(
        accepted, 0,
        "{accepted} single-bit-flip public-input mutations produced Ok(true) — security bug"
    );
}

#[test]
fn verify_cross_vector_rejects() {
    // Pair the square vk with the squares-5 proof and vice versa — any of these
    // cross-pairings must be rejected.
    let sq_vk = parse_vk(&load_from("square", "vk.bin")).unwrap();
    let sq_proof = parse_proof(&load_from("square", "proof.bin")).unwrap();
    let sq_pub = parse_public(&load_from("square", "public.bin")).unwrap();
    let sq5_vk = parse_vk(&load_from("squares-5", "vk.bin")).unwrap();
    let sq5_proof = parse_proof(&load_from("squares-5", "proof.bin")).unwrap();
    let sq5_pub = parse_public(&load_from("squares-5", "public.bin")).unwrap();

    // square proof, but with squares-5 vk → PublicInputCount error (5 ≠ 1)
    assert!(matches!(
        verify(&sq5_vk, &sq_proof, &sq_pub),
        Err(Error::PublicInputCount)
    ));

    // squares-5 proof with square vk → PublicInputCount error (1 ≠ 5)
    assert!(matches!(
        verify(&sq_vk, &sq5_proof, &sq5_pub),
        Err(Error::PublicInputCount)
    ));

    // square proof, square vk, squares-5 public → count mismatch
    assert!(matches!(
        verify(&sq_vk, &sq_proof, &sq5_pub),
        Err(Error::PublicInputCount)
    ));

    // Matching counts but wrong pairing — square proof with square vk and wrong-length-but-count-matched public.
    // (5 inputs into a 1-input vk → count mismatch caught first.)
}

#[test]
fn verify_rejects_all_zero_proof() {
    let (vk_b, _, public_b) = known_good();
    let vk = parse_vk(&vk_b).unwrap();
    let public = parse_public(&public_b).unwrap();

    // All-zero proof parses as three identity points.
    let proof = parse_proof(&vec![0u8; PROOF_SIZE]).unwrap();

    let result = verify(&vk, &proof, &public).expect("verify runs");
    assert!(!result, "all-zero proof accepted against real vk");
}
