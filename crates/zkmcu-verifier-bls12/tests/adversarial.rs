//! Adversarial / malformed-input tests for the parsers and `verify`.
//!
//! Mirrors `zkmcu-verifier/tests/adversarial.rs`. The goal is identical: prove
//! that `parse_vk`, `parse_proof`, `parse_public`, and `verify` never panic
//! on adversarial input, always reject invalid inputs, and never accept a
//! tampered proof. Any unexpected `Ok(true)` produced by a bit-flip would
//! be an exploitable finding.
//!
//! BLS12-381 / EIP-2537 specifics vs the BN254 suite:
//!   - larger wire sizes (G1 128 B, G2 256 B, proof 512 B)
//!   - Fp padding check (16-byte leading-zero prefix on every field element)
//!   - `Error::InvalidFp` replaces `Error::InvalidFq`

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

use zkmcu_verifier_bls12::{
    parse_proof, parse_public, parse_vk, verify, Error, FP_SIZE, FR_SIZE, G1_SIZE, G2_SIZE,
    MAX_NUM_IC, MAX_PUBLIC_INPUTS, PROOF_SIZE,
};

// ---- Helpers -----------------------------------------------------------

fn load_from(circuit: &str, name: &str) -> Vec<u8> {
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
    // Header valid, num_ic = u32::MAX. ic_start + num_ic * G1_SIZE must not
    // panic / overflow. Caught by the MAX_NUM_IC cap before byte-length
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
    let truncated: Vec<u8> = vk[..header + 4].to_vec();
    let parsed = parse_vk(&truncated).expect("parse zero-ic vk");
    assert_eq!(parsed.ic.len(), 0);
}

#[test]
fn parse_vk_field_element_above_modulus() {
    // Write 0xff into the first 4 bytes of alpha.x's *value* region
    // (EIP-2537 offset 16..20 inside the Fp encoding). Sets zkcrypto's
    // compression/infinity flag bits and pushes the value far above the
    // BLS12-381 base modulus; from_uncompressed must reject.
    let (mut vk, _, _) = known_good();
    for b in &mut vk[16..20] {
        *b = 0xff;
    }
    assert!(matches!(
        parse_vk(&vk),
        Err(Error::InvalidFp | Error::InvalidG1Curve | Error::InvalidG1Subgroup)
    ));
}

#[test]
fn parse_vk_fp_padding_bit_flip() {
    // Single bit flip inside the 16-byte leading-zero pad of alpha.x.
    // EIP-2537's strict-padding rule must reject with InvalidFp before the
    // value bits are even inspected.
    let (mut vk, _, _) = known_good();
    vk[5] ^= 0x01;
    assert!(matches!(parse_vk(&vk), Err(Error::InvalidFp)));
}

#[test]
fn parse_vk_all_zero_header_and_no_ic() {
    // All-zero G1 and G2 are the identity encoding in EIP-2537. These parse
    // as identity points; verify-time catches that such a VK is not valid
    // against any meaningful proof.
    let header = G1_SIZE + 3 * G2_SIZE;
    let mut bytes = vec![0u8; header + 4];
    let parsed = parse_vk(&bytes).expect("all-zero vk parses");
    assert_eq!(parsed.ic.len(), 0);

    // Claiming 1 ic entry without providing bytes → TruncatedInput.
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
    // Same trick as the VK test: 0xff into A.x's value region.
    for b in &mut proof[16..20] {
        *b = 0xff;
    }
    assert!(matches!(
        parse_proof(&proof),
        Err(Error::InvalidFp | Error::InvalidG1Curve | Error::InvalidG1Subgroup)
    ));
}

#[test]
fn parse_proof_point_not_on_curve() {
    // Build a proof with A = (x=1, y=1). BLS12-381 curve is y^2 = x^3 + 4,
    // so (1, 1): 1 ≠ 1 + 4 = 5. Not on curve.
    // In EIP-2537, x=1 is 16 zero-pad bytes + 47 zero-value bytes + 0x01.
    let mut proof = vec![0u8; PROOF_SIZE];
    proof[63] = 1; // A.x = 1 (last byte of the 64-byte Fp encoding)
    proof[127] = 1; // A.y = 1
    assert!(matches!(parse_proof(&proof), Err(Error::InvalidG1Curve)));
}

#[test]
fn parse_proof_all_zero_is_identity_points() {
    // All zeros = three identity points. Parser must accept; verify must
    // reject this against a real VK.
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
    // Claim 1 element, encode it as 32 bytes of 0xff. BLS12-381 scalar
    // modulus r is < 2^255, so 0xff..0xff is far above r; Scalar::from_bytes
    // must reject.
    let mut bytes = 1u32.to_le_bytes().to_vec();
    bytes.extend_from_slice(&[0xffu8; FR_SIZE]);
    assert!(matches!(parse_public(&bytes), Err(Error::InvalidFr)));
}

#[test]
fn parse_public_count_astronomical() {
    // count = 2^31, one Fr. Must not allocate a gigantic Vec or panic. Caught
    // by the MAX_PUBLIC_INPUTS cap, so the reported error is
    // InputLimitExceeded rather than TruncatedInput.
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
    let public: Vec<zkmcu_verifier_bls12::Fr> = Vec::new();
    assert!(matches!(
        verify(&vk, &proof, &public),
        Err(Error::PublicInputCount)
    ));
}

#[test]
fn verify_rejects_bitflip_in_every_vk_byte() {
    // Exhaustively flip every bit in the VK and confirm verify never returns
    // Ok(true). Most bytes (curve-point data, Fp-padding bytes, scalar-field
    // data) break at parse; a few still parse into a different VK and then
    // fail at verify.
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

    // Start from byte 4 to skip the count prefix; changing that just produces
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
    let sq_vk = parse_vk(&load_from("square", "vk.bin")).unwrap();
    let sq_proof = parse_proof(&load_from("square", "proof.bin")).unwrap();
    let sq_pub = parse_public(&load_from("square", "public.bin")).unwrap();
    let sq5_vk = parse_vk(&load_from("squares-5", "vk.bin")).unwrap();
    let sq5_proof = parse_proof(&load_from("squares-5", "proof.bin")).unwrap();
    let sq5_pub = parse_public(&load_from("squares-5", "public.bin")).unwrap();

    // square proof + squares-5 vk → 5 ic entries expected for 1 public input
    assert!(matches!(
        verify(&sq5_vk, &sq_proof, &sq_pub),
        Err(Error::PublicInputCount)
    ));

    // squares-5 proof + square vk → 1 ic expected for 5 public inputs
    assert!(matches!(
        verify(&sq_vk, &sq5_proof, &sq5_pub),
        Err(Error::PublicInputCount)
    ));

    // square proof + square vk + squares-5 public → count mismatch
    assert!(matches!(
        verify(&sq_vk, &sq_proof, &sq5_pub),
        Err(Error::PublicInputCount)
    ));
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

// ---- Input limits (MAX_NUM_IC, MAX_PUBLIC_INPUTS) ----------------------

#[test]
fn parse_vk_num_ic_at_limit_plus_one() {
    // num_ic = MAX_NUM_IC + 1. Buffer is padded large enough that byte-length
    // validation would pass, the InputLimitExceeded check must fire first.
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
    // num_ic = MAX_NUM_IC must still be accepted structurally. All-zero
    // header + identity ic entries yields a semantically garbage VK, but
    // parse_vk's contract is structural validity, not semantic correctness.
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

// ---- Single-point identity substitutions ------------------------------
//
// Same structure as the BN254 sibling's identity tests. Groth16 pairing
// check is `e(A, B) · e(-α, β) · e(-vk_x, γ) · e(-C, δ) = 1`. Any single
// point at identity collapses its factor to `Gt::one()` and weakens the
// equation. Parser accepts EIP-2537 all-zero G1/G2 as identity (128/256
// bytes of zeros), so we simply zero the relevant region of a known-good
// VK/proof pair and assert verify rejects.

fn zero_range(bytes: &mut [u8], start: usize, end: usize) {
    for b in bytes.iter_mut().take(end).skip(start) {
        *b = 0;
    }
}

#[test]
fn verify_rejects_proof_a_identity() {
    let (vk_b, mut proof_b, public_b) = known_good();
    zero_range(&mut proof_b, 0, G1_SIZE);
    let vk = parse_vk(&vk_b).unwrap();
    let proof = parse_proof(&proof_b).unwrap();
    let public = parse_public(&public_b).unwrap();
    let result = verify(&vk, &proof, &public).expect("verify runs");
    assert!(!result, "proof with A = identity accepted, soundness bug");
}

#[test]
fn verify_rejects_proof_b_identity() {
    let (vk_b, mut proof_b, public_b) = known_good();
    zero_range(&mut proof_b, G1_SIZE, G1_SIZE + G2_SIZE);
    let vk = parse_vk(&vk_b).unwrap();
    let proof = parse_proof(&proof_b).unwrap();
    let public = parse_public(&public_b).unwrap();
    let result = verify(&vk, &proof, &public).expect("verify runs");
    assert!(!result, "proof with B = identity accepted");
}

#[test]
fn verify_rejects_proof_c_identity() {
    let (vk_b, mut proof_b, public_b) = known_good();
    zero_range(&mut proof_b, G1_SIZE + G2_SIZE, PROOF_SIZE);
    let vk = parse_vk(&vk_b).unwrap();
    let proof = parse_proof(&proof_b).unwrap();
    let public = parse_public(&public_b).unwrap();
    let result = verify(&vk, &proof, &public).expect("verify runs");
    assert!(!result, "proof with C = identity accepted");
}

#[test]
fn verify_rejects_vk_alpha_identity() {
    let (mut vk_b, proof_b, public_b) = known_good();
    zero_range(&mut vk_b, 0, G1_SIZE);
    let vk = parse_vk(&vk_b).unwrap();
    let proof = parse_proof(&proof_b).unwrap();
    let public = parse_public(&public_b).unwrap();
    let result = verify(&vk, &proof, &public).expect("verify runs");
    assert!(!result, "VK with alpha = identity accepted");
}

#[test]
fn verify_rejects_vk_beta_identity() {
    let (mut vk_b, proof_b, public_b) = known_good();
    zero_range(&mut vk_b, G1_SIZE, G1_SIZE + G2_SIZE);
    let vk = parse_vk(&vk_b).unwrap();
    let proof = parse_proof(&proof_b).unwrap();
    let public = parse_public(&public_b).unwrap();
    let result = verify(&vk, &proof, &public).expect("verify runs");
    assert!(!result, "VK with beta = identity accepted");
}

#[test]
fn verify_rejects_vk_gamma_identity() {
    let (mut vk_b, proof_b, public_b) = known_good();
    zero_range(&mut vk_b, G1_SIZE + G2_SIZE, G1_SIZE + 2 * G2_SIZE);
    let vk = parse_vk(&vk_b).unwrap();
    let proof = parse_proof(&proof_b).unwrap();
    let public = parse_public(&public_b).unwrap();
    let result = verify(&vk, &proof, &public).expect("verify runs");
    assert!(!result, "VK with gamma = identity accepted");
}

#[test]
fn verify_rejects_vk_delta_identity() {
    let (mut vk_b, proof_b, public_b) = known_good();
    zero_range(&mut vk_b, G1_SIZE + 2 * G2_SIZE, G1_SIZE + 3 * G2_SIZE);
    let vk = parse_vk(&vk_b).unwrap();
    let proof = parse_proof(&proof_b).unwrap();
    let public = parse_public(&public_b).unwrap();
    let result = verify(&vk, &proof, &public).expect("verify runs");
    assert!(!result, "VK with delta = identity accepted");
}

#[test]
fn verify_rejects_vk_ic0_identity() {
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
    let (mut vk_b, proof_b, public_b) = known_good();
    let ic1_start = G1_SIZE + 3 * G2_SIZE + 4 + G1_SIZE;
    zero_range(&mut vk_b, ic1_start, ic1_start + G1_SIZE);
    let vk = parse_vk(&vk_b).unwrap();
    let proof = parse_proof(&proof_b).unwrap();
    let public = parse_public(&public_b).unwrap();
    let result = verify(&vk, &proof, &public).expect("verify runs");
    assert!(!result, "VK with ic[1] = identity accepted");
}

// ---- G1/G2 curve + subgroup checks (pins zkcrypto's is_on_curve +
// is_torsion_free wiring) -----------------------------------------------

#[test]
fn parse_proof_rejects_off_curve_g2() {
    // Plant B = (x = Fp2(1, 0), y = Fp2(1, 0)). Twist equation
    // y² = x³ + 4·(1+u) is not satisfied: y² = 1, x³ + b' has a non-zero
    // imaginary part since b' does. Curve-equation check fires.
    //
    // EIP-2537 G2 encoding: x.c0 ‖ x.c1 ‖ y.c0 ‖ y.c1, each a 64-byte Fp.
    // Set the last byte of x.c0 (offset B + 63) and y.c0 (offset B + 191).
    let b_off = G1_SIZE;
    let mut proof = vec![0u8; PROOF_SIZE];
    proof[b_off + FP_SIZE - 1] = 1; // B.x.c0 = 1
    proof[b_off + FP_SIZE * 3 - 1] = 1; // B.y.c0 = 1
    assert!(
        matches!(parse_proof(&proof), Err(Error::InvalidG2Curve)),
        "expected InvalidG2Curve for off-twist G2, got {:?}",
        parse_proof(&proof)
    );
}

#[test]
fn parse_proof_rejects_off_subgroup_g1() {
    // Load the host-generated on-curve-but-off-subgroup G1 fixture. Plant it
    // at A (offset 0 in the proof); B and C stay identity. The parser must
    // reject with InvalidG1Subgroup specifically, if it returns
    // InvalidG1Curve the fixture or the curve check is wrong.
    let g1_offsub = load_from("off-subgroup", "g1.bin");
    assert_eq!(g1_offsub.len(), G1_SIZE, "fixture size");

    let mut proof = vec![0u8; PROOF_SIZE];
    proof[..G1_SIZE].copy_from_slice(&g1_offsub);
    assert!(
        matches!(parse_proof(&proof), Err(Error::InvalidG1Subgroup)),
        "expected InvalidG1Subgroup, got {:?}",
        parse_proof(&proof)
    );
}

#[test]
fn parse_proof_rejects_off_subgroup_g2() {
    // Same idea, G2 fixture planted at B (offset G1_SIZE).
    let g2_offsub = load_from("off-subgroup", "g2.bin");
    assert_eq!(g2_offsub.len(), G2_SIZE, "fixture size");

    let mut proof = vec![0u8; PROOF_SIZE];
    proof[G1_SIZE..G1_SIZE + G2_SIZE].copy_from_slice(&g2_offsub);
    assert!(
        matches!(parse_proof(&proof), Err(Error::InvalidG2Subgroup)),
        "expected InvalidG2Subgroup, got {:?}",
        parse_proof(&proof)
    );
}
