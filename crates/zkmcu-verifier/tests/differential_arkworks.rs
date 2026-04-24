//! Differential tests: substrate-bn (Rust BN254 path) vs arkworks.
//!
//! Goal: for the primitive ops that Groth16 verify composes (Fq/Fr
//! arithmetic, G1/G2 addition, G1/G2 scalar multiplication), confirm that
//! substrate-bn's Rust implementation produces byte-identical results to an
//! independent reference (ark-bn254). Bugs that happen to cancel in an
//! end-to-end verify (e.g. a wrong `Fq::mul` paired with a wrong `Fq::inv`)
//! still surface here at the operation boundary.
//!
//! This file exercises the **Rust** path only, the Cortex-M33 UMAAL asm
//! path compiles only on `thumbv8m.main-none-eabihf` and is covered by a
//! separate on-device KAT fixture test (future patch). Until that lands,
//! the asm path's correctness is observable only via end-to-end Groth16
//! verify on real silicon.
//!
//! Deterministic seed so any CI failure is bit-reproducible on a dev box.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::must_use_candidate,
    clippy::similar_names
)]

use ark_bn254::{
    Fq as ArkFq, Fq2 as ArkFq2, Fr as ArkFr, G1Affine as ArkG1Aff, G1Projective as ArkG1P,
    G2Affine as ArkG2Aff, G2Projective as ArkG2P,
};
use ark_ec::{AffineRepr, CurveGroup, PrimeGroup};
use ark_ff::{BigInteger, Field, PrimeField, UniformRand};
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

use substrate_bn::{AffineG1 as BnAG1, AffineG2 as BnAG2, Fq as BnFq, Fq2 as BnFq2, Group as _};
use zkmcu_verifier::{Fr as BnFr, G1 as BnG1, G2 as BnG2};

// ---- Byte bridges ------------------------------------------------------

fn ark_fq_to_be32(f: ArkFq) -> [u8; 32] {
    let bytes = f.into_bigint().to_bytes_be();
    let mut out = [0u8; 32];
    let start = 32usize.saturating_sub(bytes.len());
    out[start..].copy_from_slice(&bytes);
    out
}

fn ark_fr_to_be32(f: ArkFr) -> [u8; 32] {
    let bytes = f.into_bigint().to_bytes_be();
    let mut out = [0u8; 32];
    let start = 32usize.saturating_sub(bytes.len());
    out[start..].copy_from_slice(&bytes);
    out
}

fn bn_fq_to_be32(f: BnFq) -> [u8; 32] {
    let mut out = [0u8; 32];
    f.to_big_endian(&mut out).unwrap();
    out
}

fn bn_fr_to_be32(f: BnFr) -> [u8; 32] {
    // substrate-bn's `Fr::to_big_endian` writes the internal Montgomery-form
    // bytes, not the integer. `Fq::to_big_endian` reverses the Montgomery
    // encoding. Go via `into_u256` so we serialise the actual scalar value.
    let mut out = [0u8; 32];
    f.into_u256().to_big_endian(&mut out).unwrap();
    out
}

fn ark_to_bn_fq(f: ArkFq) -> BnFq {
    BnFq::from_slice(&ark_fq_to_be32(f)).expect("ark Fq fits into substrate-bn Fq")
}

fn ark_to_bn_fr(f: ArkFr) -> BnFr {
    BnFr::from_slice(&ark_fr_to_be32(f)).expect("ark Fr fits into substrate-bn Fr")
}

/// EIP-197 G1 encoding: `x ‖ y`, each 32 bytes big-endian. Identity = all zeros.
fn ark_g1_to_bytes(p: &ArkG1Aff) -> [u8; 64] {
    let mut out = [0u8; 64];
    if let Some((x, y)) = p.xy() {
        out[0..32].copy_from_slice(&ark_fq_to_be32(x));
        out[32..64].copy_from_slice(&ark_fq_to_be32(y));
    }
    out
}

fn bn_g1_to_bytes(p: &BnG1) -> [u8; 64] {
    let mut out = [0u8; 64];
    if let Some(aff) = BnAG1::from_jacobian(*p) {
        out[0..32].copy_from_slice(&bn_fq_to_be32(aff.x()));
        out[32..64].copy_from_slice(&bn_fq_to_be32(aff.y()));
    }
    out
}

fn write_ark_fq2(out: &mut [u8], fq2: ArkFq2) {
    out[0..32].copy_from_slice(&ark_fq_to_be32(fq2.c1));
    out[32..64].copy_from_slice(&ark_fq_to_be32(fq2.c0));
}

fn write_bn_fq2(out: &mut [u8], fq2: BnFq2) {
    out[0..32].copy_from_slice(&bn_fq_to_be32(fq2.imaginary()));
    out[32..64].copy_from_slice(&bn_fq_to_be32(fq2.real()));
}

/// EIP-197 G2 encoding: `x.c1 ‖ x.c0 ‖ y.c1 ‖ y.c0`, each 32 bytes BE.
fn ark_g2_to_bytes(p: &ArkG2Aff) -> [u8; 128] {
    let mut out = [0u8; 128];
    if let Some((x, y)) = p.xy() {
        write_ark_fq2(&mut out[0..64], x);
        write_ark_fq2(&mut out[64..128], y);
    }
    out
}

fn bn_g2_to_bytes(p: &BnG2) -> [u8; 128] {
    let mut out = [0u8; 128];
    if let Some(aff) = BnAG2::from_jacobian(*p) {
        write_bn_fq2(&mut out[0..64], aff.x());
        write_bn_fq2(&mut out[64..128], aff.y());
    }
    out
}

/// Lift an arkworks G1 affine point to substrate-bn via its EIP-197 bytes.
/// substrate-bn's parser does curve + subgroup validation, so a bridge
/// disagreement would mean one side accepts a point the other rejects.
fn ark_to_bn_g1(p: ArkG1Aff) -> BnG1 {
    let bytes = ark_g1_to_bytes(&p);
    // Parser inline (avoid pulling zkmcu_verifier::read_g1 into scope with a
    // matching Error type): identity = (0, 0), otherwise AffineG1::new.
    if bytes.iter().all(|&b| b == 0) {
        return BnG1::zero();
    }
    let x = BnFq::from_slice(&bytes[0..32]).expect("G1 x");
    let y = BnFq::from_slice(&bytes[32..64]).expect("G1 y");
    BnG1::from(BnAG1::new(x, y).expect("ark point must pass substrate-bn checks"))
}

fn ark_to_bn_g2(p: ArkG2Aff) -> BnG2 {
    let bytes = ark_g2_to_bytes(&p);
    if bytes.iter().all(|&b| b == 0) {
        return BnG2::zero();
    }
    let x_c1 = BnFq::from_slice(&bytes[0..32]).expect("G2 x.c1");
    let x_c0 = BnFq::from_slice(&bytes[32..64]).expect("G2 x.c0");
    let y_c1 = BnFq::from_slice(&bytes[64..96]).expect("G2 y.c1");
    let y_c0 = BnFq::from_slice(&bytes[96..128]).expect("G2 y.c0");
    let x = BnFq2::new(x_c0, x_c1);
    let y = BnFq2::new(y_c0, y_c1);
    BnG2::from(BnAG2::new(x, y).expect("ark twist point must pass substrate-bn checks"))
}

// ---- Tests -------------------------------------------------------------

const SEED_FQ_MUL: u64 = 0xD1FF_F901_BA5E_0001;
const SEED_FQ_INV: u64 = 0xD1FF_F902_BA5E_0002;
const SEED_FR_MUL: u64 = 0xD1FF_F903_BA5E_0003;
const SEED_G1_ADD: u64 = 0xD1FF_F904_BA5E_0004;
const SEED_G1_MUL: u64 = 0xD1FF_F905_BA5E_0005;
const SEED_G2_ADD: u64 = 0xD1FF_F906_BA5E_0006;
const SEED_G2_MUL: u64 = 0xD1FF_F907_BA5E_0007;

#[test]
fn fq_mul_matches_arkworks() {
    // Fq multiplication is the primitive that UMAAL asm replaces on Cortex-M33.
    // A silent divergence between substrate-bn's Rust path and arkworks here
    // means one of the two Montgomery implementations has a bug, exactly the
    // class of defect that mul_reduce differential testing exists to catch.
    let mut rng = ChaCha20Rng::seed_from_u64(SEED_FQ_MUL);
    for i in 0..256 {
        let a = ArkFq::rand(&mut rng);
        let b = ArkFq::rand(&mut rng);
        let ark_prod = a * b;

        let bn_a = ark_to_bn_fq(a);
        let bn_b = ark_to_bn_fq(b);
        let bn_prod = bn_a * bn_b;

        let ark_bytes = ark_fq_to_be32(ark_prod);
        let bn_bytes = bn_fq_to_be32(bn_prod);
        assert_eq!(
            ark_bytes, bn_bytes,
            "Fq::mul mismatch at iter {i}: arkworks={ark_bytes:02x?} substrate-bn={bn_bytes:02x?}"
        );
    }
}

#[test]
fn fq_inv_matches_arkworks() {
    let mut rng = ChaCha20Rng::seed_from_u64(SEED_FQ_INV);
    for i in 0..64 {
        let a = ArkFq::rand(&mut rng);
        let ark_inv = a.inverse().expect("random Fq is invertible");

        let bn_a = ark_to_bn_fq(a);
        let bn_inv = bn_a.inverse().expect("random Fq is invertible");

        assert_eq!(
            ark_fq_to_be32(ark_inv),
            bn_fq_to_be32(bn_inv),
            "Fq::inverse mismatch at iter {i}"
        );
    }
}

#[test]
fn fr_mul_matches_arkworks() {
    let mut rng = ChaCha20Rng::seed_from_u64(SEED_FR_MUL);
    for i in 0..256 {
        let a = ArkFr::rand(&mut rng);
        let b = ArkFr::rand(&mut rng);
        let ark_prod = a * b;

        let bn_a = ark_to_bn_fr(a);
        let bn_b = ark_to_bn_fr(b);
        let bn_prod = bn_a * bn_b;

        assert_eq!(
            ark_fr_to_be32(ark_prod),
            bn_fr_to_be32(bn_prod),
            "Fr::mul mismatch at iter {i}"
        );
    }
}

#[test]
fn g1_add_matches_arkworks() {
    let mut rng = ChaCha20Rng::seed_from_u64(SEED_G1_ADD);
    let g = ArkG1P::generator();
    for i in 0..64 {
        let s1 = ArkFr::rand(&mut rng);
        let s2 = ArkFr::rand(&mut rng);
        let p1 = (g * s1).into_affine();
        let p2 = (g * s2).into_affine();
        let ark_sum = (p1 + p2).into_affine();

        let bn_p1 = ark_to_bn_g1(p1);
        let bn_p2 = ark_to_bn_g1(p2);
        let bn_sum = bn_p1 + bn_p2;

        assert_eq!(
            ark_g1_to_bytes(&ark_sum),
            bn_g1_to_bytes(&bn_sum),
            "G1 addition mismatch at iter {i}"
        );
    }
}

#[test]
fn g1_scalar_mul_matches_arkworks() {
    let mut rng = ChaCha20Rng::seed_from_u64(SEED_G1_MUL);
    let g = ArkG1P::generator();
    for i in 0..32 {
        // Random *base* point (not the generator, scalar mul of generator is
        // a well-trodden happy path; we want to exercise arbitrary G1 inputs).
        let base_scalar = ArkFr::rand(&mut rng);
        let scalar = ArkFr::rand(&mut rng);
        let base_ark = (g * base_scalar).into_affine();
        let ark_out = (base_ark * scalar).into_affine();

        let base_bn = ark_to_bn_g1(base_ark);
        let scalar_bn = ark_to_bn_fr(scalar);
        let bn_out = base_bn * scalar_bn;

        assert_eq!(
            ark_g1_to_bytes(&ark_out),
            bn_g1_to_bytes(&bn_out),
            "G1 scalar mul mismatch at iter {i}"
        );
    }
}

#[test]
fn g2_add_matches_arkworks() {
    let mut rng = ChaCha20Rng::seed_from_u64(SEED_G2_ADD);
    let g = ArkG2P::generator();
    for i in 0..32 {
        let s1 = ArkFr::rand(&mut rng);
        let s2 = ArkFr::rand(&mut rng);
        let p1 = (g * s1).into_affine();
        let p2 = (g * s2).into_affine();
        let ark_sum = (p1 + p2).into_affine();

        let bn_p1 = ark_to_bn_g2(p1);
        let bn_p2 = ark_to_bn_g2(p2);
        let bn_sum = bn_p1 + bn_p2;

        assert_eq!(
            ark_g2_to_bytes(&ark_sum),
            bn_g2_to_bytes(&bn_sum),
            "G2 addition mismatch at iter {i}"
        );
    }
}

#[test]
fn g2_scalar_mul_matches_arkworks() {
    let mut rng = ChaCha20Rng::seed_from_u64(SEED_G2_MUL);
    let g = ArkG2P::generator();
    for i in 0..16 {
        let base_scalar = ArkFr::rand(&mut rng);
        let scalar = ArkFr::rand(&mut rng);
        let base_ark = (g * base_scalar).into_affine();
        let ark_out = (base_ark * scalar).into_affine();

        let base_bn = ark_to_bn_g2(base_ark);
        let scalar_bn = ark_to_bn_fr(scalar);
        let bn_out = base_bn * scalar_bn;

        assert_eq!(
            ark_g2_to_bytes(&ark_out),
            bn_g2_to_bytes(&bn_out),
            "G2 scalar mul mismatch at iter {i}"
        );
    }
}
