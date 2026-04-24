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
    parse_proof, parse_public, parse_vk, verify, Error, FR_SIZE, G1_SIZE, G2_SIZE, MAX_NUM_IC,
    MAX_PUBLIC_INPUTS, PROOF_SIZE,
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
    // panic / overflow. Now caught by the MAX_NUM_IC cap before byte-length
    // validation runs (u32::MAX ≫ 1024), so the reported error is
    // InputLimitExceeded rather than TruncatedInput.
    let (mut vk, _, _) = known_good();
    let header = G1_SIZE + 3 * G2_SIZE;
    vk[header..header + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(parse_vk(&vk), Err(Error::InputLimitExceeded)));
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
                   // B can stay zero (identity) and C zero, doesn't matter, parser rejects A first.
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
    // Claim 1 element, encode it as 32 bytes of 0xff, exceeds scalar modulus.
    let mut bytes = 1u32.to_le_bytes().to_vec();
    bytes.extend_from_slice(&[0xffu8; FR_SIZE]);
    assert!(matches!(parse_public(&bytes), Err(Error::InvalidFr)));
}

#[test]
fn parse_public_count_astronomical() {
    // Claim 2^31 elements, provide 32 bytes. Must not allocate a gigantic Vec
    // or panic. Now caught by the MAX_PUBLIC_INPUTS cap, so the reported error
    // is InputLimitExceeded rather than TruncatedInput.
    let mut bytes = 0x8000_0000u32.to_le_bytes().to_vec();
    bytes.extend_from_slice(&[0u8; FR_SIZE]);
    assert!(matches!(
        parse_public(&bytes),
        Err(Error::InputLimitExceeded)
    ));
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
    // We either get a parse error (most bytes, any tamper to curve points breaks them)
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
        "{accepted} single-bit-flip VK mutations produced Ok(true), this is a security bug. \
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
        "{accepted} single-bit-flip proof mutations produced Ok(true), security bug"
    );
}

#[test]
fn verify_rejects_bitflip_in_every_public_byte() {
    let (vk_b, proof_b, public_b) = known_good();
    let vk = parse_vk(&vk_b).unwrap();
    let proof = parse_proof(&proof_b).unwrap();

    let mut accepted = 0usize;

    // Start from byte 4 to skip the count prefix, changing it just produces
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
        "{accepted} single-bit-flip public-input mutations produced Ok(true), security bug"
    );
}

#[test]
fn verify_cross_vector_rejects() {
    // Pair the square vk with the squares-5 proof and vice versa, any of these
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

    // Matching counts but wrong pairing, square proof with square vk and wrong-length-but-count-matched public.
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

// ---- G2 subgroup check (pins substrate-bn's check_order behaviour) -----

// ---- Strict-length (reject trailing bytes) ----------------------------

#[test]
fn parse_vk_rejects_trailing_byte() {
    let (mut vk, _, _) = known_good();
    vk.push(0x00);
    assert!(matches!(parse_vk(&vk), Err(Error::TruncatedInput)));
}

#[test]
fn parse_proof_rejects_trailing_byte() {
    let (_, mut proof, _) = known_good();
    proof.push(0x00);
    assert!(matches!(parse_proof(&proof), Err(Error::TruncatedInput)));
}

#[test]
fn parse_public_rejects_trailing_byte() {
    let (_, _, mut public) = known_good();
    public.push(0x00);
    assert!(matches!(parse_public(&public), Err(Error::TruncatedInput)));
}

// ---- Single-point identity substitutions (each verify pair weakens) ---
//
// The Groth16 pairing check is `e(-A, B) · e(α, β) · e(vk_x, γ) · e(C, δ) = 1`.
// If any single point on either side is the identity, that factor collapses
// to `Gt::one()` and the equation becomes weaker, in the limit, one
// missing factor means the remaining three need to satisfy a different
// relation, which an attacker could potentially engineer for. These tests
// pin substrate-bn's pairing behaviour on all identity placements: verify
// must reject a known-good VK/proof pair after any single identity swap.

fn zero_range(bytes: &mut [u8], start: usize, end: usize) {
    for b in bytes.iter_mut().take(end).skip(start) {
        *b = 0;
    }
}

#[test]
fn verify_rejects_proof_a_identity() {
    let (vk_b, mut proof_b, public_b) = known_good();
    zero_range(&mut proof_b, 0, G1_SIZE); // A = G1 identity
    let vk = parse_vk(&vk_b).unwrap();
    let proof = parse_proof(&proof_b).unwrap();
    let public = parse_public(&public_b).unwrap();
    let result = verify(&vk, &proof, &public).expect("verify runs");
    assert!(!result, "proof with A = identity accepted, soundness bug");
}

#[test]
fn verify_rejects_proof_b_identity() {
    let (vk_b, mut proof_b, public_b) = known_good();
    zero_range(&mut proof_b, G1_SIZE, G1_SIZE + G2_SIZE); // B = G2 identity
    let vk = parse_vk(&vk_b).unwrap();
    let proof = parse_proof(&proof_b).unwrap();
    let public = parse_public(&public_b).unwrap();
    let result = verify(&vk, &proof, &public).expect("verify runs");
    assert!(!result, "proof with B = identity accepted, soundness bug");
}

#[test]
fn verify_rejects_proof_c_identity() {
    let (vk_b, mut proof_b, public_b) = known_good();
    zero_range(&mut proof_b, G1_SIZE + G2_SIZE, PROOF_SIZE); // C = G1 identity
    let vk = parse_vk(&vk_b).unwrap();
    let proof = parse_proof(&proof_b).unwrap();
    let public = parse_public(&public_b).unwrap();
    let result = verify(&vk, &proof, &public).expect("verify runs");
    assert!(!result, "proof with C = identity accepted, soundness bug");
}

#[test]
fn verify_rejects_vk_alpha_identity() {
    let (mut vk_b, proof_b, public_b) = known_good();
    zero_range(&mut vk_b, 0, G1_SIZE); // alpha = G1 identity
    let vk = parse_vk(&vk_b).unwrap();
    let proof = parse_proof(&proof_b).unwrap();
    let public = parse_public(&public_b).unwrap();
    let result = verify(&vk, &proof, &public).expect("verify runs");
    assert!(
        !result,
        "VK with alpha = identity accepted, tampered VK soundness bug"
    );
}

#[test]
fn verify_rejects_vk_beta_identity() {
    let (mut vk_b, proof_b, public_b) = known_good();
    zero_range(&mut vk_b, G1_SIZE, G1_SIZE + G2_SIZE); // beta = G2 identity
    let vk = parse_vk(&vk_b).unwrap();
    let proof = parse_proof(&proof_b).unwrap();
    let public = parse_public(&public_b).unwrap();
    let result = verify(&vk, &proof, &public).expect("verify runs");
    assert!(!result, "VK with beta = identity accepted");
}

#[test]
fn verify_rejects_vk_gamma_identity() {
    let (mut vk_b, proof_b, public_b) = known_good();
    zero_range(&mut vk_b, G1_SIZE + G2_SIZE, G1_SIZE + 2 * G2_SIZE); // gamma
    let vk = parse_vk(&vk_b).unwrap();
    let proof = parse_proof(&proof_b).unwrap();
    let public = parse_public(&public_b).unwrap();
    let result = verify(&vk, &proof, &public).expect("verify runs");
    assert!(!result, "VK with gamma = identity accepted");
}

#[test]
fn verify_rejects_vk_delta_identity() {
    let (mut vk_b, proof_b, public_b) = known_good();
    zero_range(&mut vk_b, G1_SIZE + 2 * G2_SIZE, G1_SIZE + 3 * G2_SIZE); // delta
    let vk = parse_vk(&vk_b).unwrap();
    let proof = parse_proof(&proof_b).unwrap();
    let public = parse_public(&public_b).unwrap();
    let result = verify(&vk, &proof, &public).expect("verify runs");
    assert!(!result, "VK with delta = identity accepted");
}

#[test]
fn verify_rejects_vk_ic0_identity() {
    // ic[0] is the constant term of `vk_x = ic[0] + Σ public[i] * ic[i+1]`.
    // Zeroing it turns vk_x into just `public[0] * ic[1]`.
    let (mut vk_b, proof_b, public_b) = known_good();
    let ic0_start = G1_SIZE + 3 * G2_SIZE + 4;
    zero_range(&mut vk_b, ic0_start, ic0_start + G1_SIZE);
    let vk = parse_vk(&vk_b).unwrap();
    let proof = parse_proof(&proof_b).unwrap();
    let public = parse_public(&public_b).unwrap();
    let result = verify(&vk, &proof, &public).expect("verify runs");
    assert!(!result, "VK with ic[0] = identity accepted");
}

#[test]
fn verify_rejects_vk_ic1_identity() {
    // ic[1] is the coefficient for the first public input.
    // Zeroing it kills the public input's contribution to vk_x.
    let (mut vk_b, proof_b, public_b) = known_good();
    let ic1_start = G1_SIZE + 3 * G2_SIZE + 4 + G1_SIZE;
    zero_range(&mut vk_b, ic1_start, ic1_start + G1_SIZE);
    let vk = parse_vk(&vk_b).unwrap();
    let proof = parse_proof(&proof_b).unwrap();
    let public = parse_public(&public_b).unwrap();
    let result = verify(&vk, &proof, &public).expect("verify runs");
    assert!(!result, "VK with ic[1] = identity accepted");
}

#[test]
fn parse_proof_rejects_off_curve_g2() {
    // Build a G2 point B = (x, y) with x = 1 (as Fq2 (1, 0)) and y = 1 (same).
    // Twist equation for BN254 G2 is y² = x³ + b' with b' ≠ 0, so
    // (1)² = 1 ≠ 1 + b' = x³ + b'. Curve-equation check fires first.
    //
    // EIP-197 G2 encoding is x.c1 ‖ x.c0 ‖ y.c1 ‖ y.c0, 32B big-endian each,
    // so setting byte index 63 (last byte of x.c0) and index 127 (last byte of
    // y.c0) to 1 gives us (Fq2(1,0), Fq2(1,0)). The all-zero identity shortcut
    // does not fire because (x, y) ≠ (0, 0).
    let mut proof = vec![0u8; PROOF_SIZE];
    proof[G1_SIZE + 63] = 1;
    proof[G1_SIZE + 127] = 1;
    assert!(
        matches!(parse_proof(&proof), Err(Error::InvalidG2Curve)),
        "expected InvalidG2Curve for off-twist G2, got {:?}",
        parse_proof(&proof)
    );
}

#[test]
fn parse_proof_rejects_off_subgroup_g2() {
    // Pin substrate-bn's `G2Params::check_order()` behaviour: a point that is
    // on the twist but outside the order-r subgroup must be rejected with
    // InvalidG2Subgroup. If upstream ever drops the subgroup check, or if we
    // accidentally bypass AffineG2::new, this test fails loudly.
    //
    // Construction: enumerate small Fq2 x values, compute y² = x³ + b', take
    // sqrt. A twist point is in the r-subgroup with probability 1/h where h
    // is the twist cofactor, ≈ 2^254 for BN254 G2. So the first valid (x, y)
    // we find is off-subgroup with overwhelming probability.
    use substrate_bn::{Fq, Fq2, G2};

    let b_prime = G2::b();

    let mut x_c0 = Fq::zero();
    let mut proof = vec![0u8; PROOF_SIZE];

    for _ in 0..10_000 {
        x_c0 = x_c0 + Fq::one();
        let x = Fq2::new(x_c0, Fq::zero());
        let rhs = x * x * x + b_prime;
        let Some(y) = rhs.sqrt() else { continue };

        // Serialize as EIP-197 G2 into proof[G1_SIZE .. G1_SIZE + G2_SIZE].
        let b_off = G1_SIZE;
        x.imaginary()
            .to_big_endian(&mut proof[b_off..b_off + 32])
            .unwrap();
        x.real()
            .to_big_endian(&mut proof[b_off + 32..b_off + 64])
            .unwrap();
        y.imaginary()
            .to_big_endian(&mut proof[b_off + 64..b_off + 96])
            .unwrap();
        y.real()
            .to_big_endian(&mut proof[b_off + 96..b_off + 128])
            .unwrap();

        match parse_proof(&proof) {
            Err(Error::InvalidG2Subgroup) => return, // test passes
            Ok(_) => {}                              // freak hit on subgroup, try next x
            Err(e) => panic!("unexpected parse error for off-subgroup G2: {e:?}"),
        }
    }

    panic!(
        "could not construct an off-subgroup G2 point in 10_000 iterations. \
         Either the twist coefficient is wrong, the subgroup check regressed \
         (silently accepting off-subgroup points), or the cofactor / prob math \
         is off."
    );
}

// ---- Input limits (MAX_NUM_IC, MAX_PUBLIC_INPUTS) ----------------------

#[test]
fn parse_vk_num_ic_at_limit_plus_one() {
    // num_ic = MAX_NUM_IC + 1. Buffer is padded so byte-length validation
    // would pass, the InputLimitExceeded check must fire first.
    let header = G1_SIZE + 3 * G2_SIZE;
    let over = u32::try_from(MAX_NUM_IC + 1).unwrap();
    let body_size = header + 4 + (over as usize) * G1_SIZE;
    let mut vk = vec![0u8; body_size];
    // Header can be all zeros; only the num_ic field matters for this test.
    vk[header..header + 4].copy_from_slice(&over.to_le_bytes());
    assert!(matches!(parse_vk(&vk), Err(Error::InputLimitExceeded)));
}

#[test]
fn parse_vk_num_ic_at_limit_parses() {
    // num_ic = MAX_NUM_IC must still be accepted structurally (we don't want
    // a near-boundary off-by-one that rejects legal VKs). Use all-zero header
    // + identity ic entries; VK is a garbage identity-only VK but parse_vk's
    // contract is structural validity, not semantic correctness.
    let header = G1_SIZE + 3 * G2_SIZE;
    let at = u32::try_from(MAX_NUM_IC).unwrap();
    let body_size = header + 4 + (at as usize) * G1_SIZE;
    let mut vk = vec![0u8; body_size];
    vk[header..header + 4].copy_from_slice(&at.to_le_bytes());
    let parsed = parse_vk(&vk).expect("num_ic = MAX_NUM_IC must parse");
    assert_eq!(parsed.ic.len(), MAX_NUM_IC);
}

#[test]
fn parse_public_count_at_limit_plus_one() {
    let over = u32::try_from(MAX_PUBLIC_INPUTS + 1).unwrap();
    let mut bytes = over.to_le_bytes().to_vec();
    bytes.extend(vec![0u8; (over as usize) * FR_SIZE]);
    assert!(matches!(
        parse_public(&bytes),
        Err(Error::InputLimitExceeded)
    ));
}

#[test]
fn parse_public_count_at_limit_parses() {
    // count = MAX_PUBLIC_INPUTS with all-zero Fr values (zero is canonical).
    let at = u32::try_from(MAX_PUBLIC_INPUTS).unwrap();
    let mut bytes = at.to_le_bytes().to_vec();
    bytes.extend(vec![0u8; (at as usize) * FR_SIZE]);
    let parsed = parse_public(&bytes).expect("count = MAX_PUBLIC_INPUTS must parse");
    assert_eq!(parsed.len(), MAX_PUBLIC_INPUTS);
}
