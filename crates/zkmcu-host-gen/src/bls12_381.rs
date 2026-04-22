//! BLS12-381 vector generation (EIP-2537 wire format).
//!
//! Produces `crates/zkmcu-vectors/data/bls12-381/{square,squares-5}/{vk,proof,public}.bin`.
//!
//! Each vector is cross-checked on host before the bytes hit disk: the arkworks
//! Groth16 verifier must accept the proof, AND the EIP-2537 bytes must
//! round-trip through the zkcrypto `bls12_381` crate and pass a
//! hand-rolled Groth16 pairing check. If either side disagrees the build
//! aborts — catches wire-format bugs (Fp padding, Fp2 byte order)
//! before the bytes pollute the committed .bin files.
//!
//! ## Wire format (EIP-2537)
//!
//! - Fp: 64 bytes. 16 zero bytes + 48 bytes big-endian value.
//! - Fp2: 128 bytes, order `(c0 ‖ c1)` — *opposite* of EIP-197's BN254 order.
//! - G1: 128 bytes, `x ‖ y`.
//! - G2: 256 bytes, `x ‖ y` where each is Fp2, so `x.c0 ‖ x.c1 ‖ y.c0 ‖ y.c1`.
//! - Point at infinity: all zeros.

use std::fs;
use std::path::Path;

use ark_bls12_381::{Bls12_381, Fq, Fr};
use ark_ec::AffineRepr;
use ark_ff::{BigInteger, PrimeField, Zero};
use ark_groth16::Groth16;
use ark_snark::SNARK;
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha20Rng;

use crate::circuits::{SquareCircuit, SquaresNCircuit};

// ---- EIP-2537 serialization (arkworks → bytes) -------------------------

const FP_SIZE: usize = 64; // 16 zero bytes + 48 bytes BE
const FR_SIZE: usize = 32;
const G1_SIZE: usize = FP_SIZE * 2; // 128
const G2_SIZE: usize = FP_SIZE * 4; // 256

fn fq_to_eip2537(f: Fq) -> [u8; FP_SIZE] {
    let bytes = f.into_bigint().to_bytes_be();
    let mut out = [0u8; FP_SIZE];
    let start = FP_SIZE.saturating_sub(bytes.len());
    out.get_mut(start..)
        .expect("Fq serialization fits in 64 bytes (48 value + 16 pad)")
        .copy_from_slice(&bytes);
    out
}

fn fr_to_be32(f: Fr) -> [u8; FR_SIZE] {
    let bytes = f.into_bigint().to_bytes_be();
    let mut out = [0u8; FR_SIZE];
    let start = FR_SIZE.saturating_sub(bytes.len());
    out.get_mut(start..)
        .expect("Fr serialization fits in 32 bytes")
        .copy_from_slice(&bytes);
    out
}

fn g1_to_eip2537(p: &ark_bls12_381::G1Affine) -> [u8; G1_SIZE] {
    let mut out = [0u8; G1_SIZE];
    if let Some((x, y)) = p.xy() {
        let (head, tail) = out.split_at_mut(FP_SIZE);
        head.copy_from_slice(&fq_to_eip2537(x));
        tail.copy_from_slice(&fq_to_eip2537(y));
    }
    out
}

fn g2_to_eip2537(p: &ark_bls12_381::G2Affine) -> [u8; G2_SIZE] {
    let mut out = [0u8; G2_SIZE];
    if let Some((x, y)) = p.xy() {
        // EIP-2537 Fp2 order: c0 first, c1 second.
        // (Opposite of EIP-197's BN254 convention.)
        let (xs, ys) = out.split_at_mut(FP_SIZE * 2);
        let (x_c0, x_c1) = xs.split_at_mut(FP_SIZE);
        x_c0.copy_from_slice(&fq_to_eip2537(x.c0));
        x_c1.copy_from_slice(&fq_to_eip2537(x.c1));
        let (y_c0, y_c1) = ys.split_at_mut(FP_SIZE);
        y_c0.copy_from_slice(&fq_to_eip2537(y.c0));
        y_c1.copy_from_slice(&fq_to_eip2537(y.c1));
    }
    out
}

fn u32_len(n: usize) -> [u8; 4] {
    u32::try_from(n)
        .expect("collection length fits in u32 for EIP-2537 vk/proof/public")
        .to_le_bytes()
}

fn encode_vk(vk: &ark_groth16::VerifyingKey<Bls12_381>) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&g1_to_eip2537(&vk.alpha_g1));
    out.extend_from_slice(&g2_to_eip2537(&vk.beta_g2));
    out.extend_from_slice(&g2_to_eip2537(&vk.gamma_g2));
    out.extend_from_slice(&g2_to_eip2537(&vk.delta_g2));
    out.extend_from_slice(&u32_len(vk.gamma_abc_g1.len()));
    for ic in &vk.gamma_abc_g1 {
        out.extend_from_slice(&g1_to_eip2537(ic));
    }
    out
}

fn encode_proof(proof: &ark_groth16::Proof<Bls12_381>) -> Vec<u8> {
    let mut out = Vec::with_capacity(G1_SIZE + G2_SIZE + G1_SIZE);
    out.extend_from_slice(&g1_to_eip2537(&proof.a));
    out.extend_from_slice(&g2_to_eip2537(&proof.b));
    out.extend_from_slice(&g1_to_eip2537(&proof.c));
    out
}

fn encode_public(public: &[Fr]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + public.len() * FR_SIZE);
    out.extend_from_slice(&u32_len(public.len()));
    for p in public {
        out.extend_from_slice(&fr_to_be32(*p));
    }
    out
}

// ---- Cross-check: EIP-2537 bytes → zkcrypto `bls12_381` types ----------
//
// The zkcrypto crate uses its own wire format for `from_uncompressed`:
// 96 bytes for G1 (48+48, big-endian) and 192 bytes for G2 with Fp2 order
// `(c1, c0)` *not* `(c0, c1)`. Bridging from EIP-2537 means stripping the
// 16-byte padding and, for G2, swapping the Fp2 coefficient order.

mod zkc_verify {
    use bls12_381 as bls;
    use group::{Curve, Group};
    use pairing::MultiMillerLoop;

    use super::{FP_SIZE, FR_SIZE, G1_SIZE, G2_SIZE};

    fn strip_padding(fp: &[u8]) -> Option<[u8; 48]> {
        if fp.len() != FP_SIZE {
            return None;
        }
        if fp.get(..16)?.iter().any(|&b| b != 0) {
            return None;
        }
        let mut raw = [0u8; 48];
        raw.copy_from_slice(fp.get(16..FP_SIZE)?);
        Some(raw)
    }

    pub fn parse_g1(bytes: &[u8]) -> Option<bls::G1Affine> {
        if bytes.len() != G1_SIZE {
            return None;
        }
        // All-zero encodes the identity in EIP-2537.
        if bytes.iter().all(|&b| b == 0) {
            return Some(bls::G1Affine::identity());
        }
        let x = strip_padding(bytes.get(..FP_SIZE)?)?;
        let y = strip_padding(bytes.get(FP_SIZE..G1_SIZE)?)?;
        // zkcrypto G1 uncompressed: x (48 BE) || y (48 BE).
        let mut zkc = [0u8; 96];
        zkc.get_mut(..48)?.copy_from_slice(&x);
        zkc.get_mut(48..)?.copy_from_slice(&y);
        bls::G1Affine::from_uncompressed(&zkc).into()
    }

    pub fn parse_g2(bytes: &[u8]) -> Option<bls::G2Affine> {
        if bytes.len() != G2_SIZE {
            return None;
        }
        if bytes.iter().all(|&b| b == 0) {
            return Some(bls::G2Affine::identity());
        }
        // EIP-2537 G2 = x.c0 ‖ x.c1 ‖ y.c0 ‖ y.c1. Each is 64 bytes (padded Fp).
        let xc0 = strip_padding(bytes.get(0..FP_SIZE)?)?;
        let xc1 = strip_padding(bytes.get(FP_SIZE..FP_SIZE * 2)?)?;
        let yc0 = strip_padding(bytes.get(FP_SIZE * 2..FP_SIZE * 3)?)?;
        let yc1 = strip_padding(bytes.get(FP_SIZE * 3..G2_SIZE)?)?;
        // zkcrypto G2 uncompressed: x.c1 ‖ x.c0 ‖ y.c1 ‖ y.c0, each 48 BE.
        let mut zkc = [0u8; 192];
        zkc.get_mut(0..48)?.copy_from_slice(&xc1);
        zkc.get_mut(48..96)?.copy_from_slice(&xc0);
        zkc.get_mut(96..144)?.copy_from_slice(&yc1);
        zkc.get_mut(144..192)?.copy_from_slice(&yc0);
        bls::G2Affine::from_uncompressed(&zkc).into()
    }

    pub fn parse_fr(bytes: &[u8]) -> Option<bls::Scalar> {
        if bytes.len() != FR_SIZE {
            return None;
        }
        // zkcrypto Scalar::from_bytes expects little-endian; public-input wire
        // format is big-endian (matches ark's to_bytes_be()).
        let mut le = [0u8; 32];
        for (i, &b) in bytes.iter().enumerate() {
            *le.get_mut(31 - i)? = b;
        }
        bls::Scalar::from_bytes(&le).into()
    }

    pub struct Vk {
        pub alpha_g1: bls::G1Affine,
        pub beta_g2: bls::G2Affine,
        pub gamma_g2: bls::G2Affine,
        pub delta_g2: bls::G2Affine,
        pub ic: Vec<bls::G1Affine>,
    }

    pub struct Proof {
        pub a: bls::G1Affine,
        pub b: bls::G2Affine,
        pub c: bls::G1Affine,
    }

    pub fn parse_vk(bytes: &[u8]) -> Option<Vk> {
        let alpha_g1 = parse_g1(bytes.get(0..G1_SIZE)?)?;
        let mut off = G1_SIZE;
        let beta_g2 = parse_g2(bytes.get(off..off + G2_SIZE)?)?;
        off += G2_SIZE;
        let gamma_g2 = parse_g2(bytes.get(off..off + G2_SIZE)?)?;
        off += G2_SIZE;
        let delta_g2 = parse_g2(bytes.get(off..off + G2_SIZE)?)?;
        off += G2_SIZE;
        let mut num_ic = [0u8; 4];
        num_ic.copy_from_slice(bytes.get(off..off + 4)?);
        let num_ic = u32::from_le_bytes(num_ic) as usize;
        off += 4;
        let mut ic = Vec::with_capacity(num_ic);
        for _ in 0..num_ic {
            ic.push(parse_g1(bytes.get(off..off + G1_SIZE)?)?);
            off += G1_SIZE;
        }
        Some(Vk {
            alpha_g1,
            beta_g2,
            gamma_g2,
            delta_g2,
            ic,
        })
    }

    pub fn parse_proof(bytes: &[u8]) -> Option<Proof> {
        let a = parse_g1(bytes.get(0..G1_SIZE)?)?;
        let b = parse_g2(bytes.get(G1_SIZE..G1_SIZE + G2_SIZE)?)?;
        let c = parse_g1(bytes.get(G1_SIZE + G2_SIZE..G1_SIZE + G2_SIZE + G1_SIZE)?)?;
        Some(Proof { a, b, c })
    }

    pub fn parse_public(bytes: &[u8]) -> Option<Vec<bls::Scalar>> {
        let mut len = [0u8; 4];
        len.copy_from_slice(bytes.get(..4)?);
        let n = u32::from_le_bytes(len) as usize;
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let start = 4 + i * FR_SIZE;
            out.push(parse_fr(bytes.get(start..start + FR_SIZE)?)?);
        }
        Some(out)
    }

    /// Hand-rolled Groth16 pairing check over zkcrypto `bls12_381` types:
    /// `e(A, B) · e(-α, β) · e(-vk_x, γ) · e(-C, δ) == 1`
    /// where `vk_x = IC[0] + Σ public[i] · IC[i+1]`.
    pub fn verify(vk: &Vk, proof: &Proof, public: &[bls::Scalar]) -> bool {
        if vk.ic.len() != public.len() + 1 {
            return false;
        }
        let Some((ic0, ic_rest)) = vk.ic.split_first() else {
            return false;
        };
        let mut acc = bls::G1Projective::from(*ic0);
        for (scalar, ic_i) in public.iter().zip(ic_rest.iter()) {
            acc += *ic_i * scalar;
        }
        let vk_x = acc.to_affine();

        let neg_alpha = (-bls::G1Projective::from(vk.alpha_g1)).to_affine();
        let neg_vk_x = (-bls::G1Projective::from(vk_x)).to_affine();
        let neg_c = (-bls::G1Projective::from(proof.c)).to_affine();

        let b_prep = bls::G2Prepared::from(proof.b);
        let beta_prep = bls::G2Prepared::from(vk.beta_g2);
        let gamma_prep = bls::G2Prepared::from(vk.gamma_g2);
        let delta_prep = bls::G2Prepared::from(vk.delta_g2);

        let result = bls::Bls12::multi_miller_loop(&[
            (&proof.a, &b_prep),
            (&neg_alpha, &beta_prep),
            (&neg_vk_x, &gamma_prep),
            (&neg_c, &delta_prep),
        ])
        .final_exponentiation();

        bool::from(result.is_identity())
    }
}

/// Verify the freshly-emitted EIP-2537 bytes round-trip cleanly through
/// zkcrypto's `bls12_381` crate. Panics on disagreement: we refuse to write
/// vectors that two independent stacks disagree on.
fn cross_check_zkcrypto(vk_bytes: &[u8], proof_bytes: &[u8], public_bytes: &[u8]) {
    let vk = zkc_verify::parse_vk(vk_bytes)
        .expect("zkcrypto failed to parse EIP-2537 VK bytes written by arkworks");
    let proof = zkc_verify::parse_proof(proof_bytes)
        .expect("zkcrypto failed to parse EIP-2537 proof bytes written by arkworks");
    let public = zkc_verify::parse_public(public_bytes)
        .expect("zkcrypto failed to parse EIP-2537 public-input bytes written by arkworks");
    assert!(
        zkc_verify::verify(&vk, &proof, &public),
        "EIP-2537 cross-check failed: arkworks says ok, zkcrypto says reject. \
         Wire format bug (likely Fp2 order or Fp padding) — refusing to write vectors."
    );
}

// ---- Vector generation -------------------------------------------------

fn generate_square(out_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let dir = out_root.join("square");
    fs::create_dir_all(&dir)?;

    let mut rng = ChaCha20Rng::seed_from_u64(0xB15C_0DE1);

    let x = Fr::from(3u64);
    let y = x * x;

    let setup_circuit = SquareCircuit::<Fr> { x: None, y: None };
    let (pk, vk) = Groth16::<Bls12_381>::circuit_specific_setup(setup_circuit, &mut rng)?;

    let proof_circuit = SquareCircuit::<Fr> {
        x: Some(x),
        y: Some(y),
    };
    let proof = Groth16::<Bls12_381>::prove(&pk, proof_circuit, &mut rng)?;

    let native_ok = Groth16::<Bls12_381>::verify(&vk, &[y], &proof)?;
    assert!(
        native_ok,
        "native arkworks verify failed (BLS12-381 square) — refusing to write bad vectors"
    );

    let vk_bytes = encode_vk(&vk);
    let proof_bytes = encode_proof(&proof);
    let public_bytes = encode_public(&[y]);

    cross_check_zkcrypto(&vk_bytes, &proof_bytes, &public_bytes);

    fs::write(dir.join("vk.bin"), &vk_bytes)?;
    fs::write(dir.join("proof.bin"), &proof_bytes)?;
    fs::write(dir.join("public.bin"), &public_bytes)?;

    println!(
        "wrote bls12-381/square/ vk={} B proof={} B public={} B (arkworks + zkcrypto both ok)",
        vk_bytes.len(),
        proof_bytes.len(),
        public_bytes.len()
    );

    assert_eq!(vk.gamma_abc_g1.len(), 2);
    assert!(!Fr::is_zero(&y));

    Ok(())
}

fn generate_squares_n<const N: usize>(
    out_root: &Path,
    slug: &str,
    seed: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = out_root.join(slug);
    fs::create_dir_all(&dir)?;

    let mut rng = ChaCha20Rng::seed_from_u64(seed);

    let mut xs = [Fr::from(0u64); N];
    for (i, v) in xs.iter_mut().enumerate() {
        *v = Fr::from((i as u64 + 3) * 17 + 1);
    }
    let mut ys = [Fr::from(0u64); N];
    for (y, x) in ys.iter_mut().zip(xs.iter()) {
        *y = *x * *x;
    }

    let setup_circuit = SquaresNCircuit::<Fr, N> { xs: None, ys: None };
    let (pk, vk) = Groth16::<Bls12_381>::circuit_specific_setup(setup_circuit, &mut rng)?;

    let proof_circuit = SquaresNCircuit::<Fr, N> {
        xs: Some(xs),
        ys: Some(ys),
    };
    let proof = Groth16::<Bls12_381>::prove(&pk, proof_circuit, &mut rng)?;

    let native_ok = Groth16::<Bls12_381>::verify(&vk, &ys, &proof)?;
    assert!(
        native_ok,
        "native arkworks verify failed (BLS12-381 {slug}) — refusing to write bad vectors"
    );

    let vk_bytes = encode_vk(&vk);
    let proof_bytes = encode_proof(&proof);
    let public_bytes = encode_public(&ys);

    cross_check_zkcrypto(&vk_bytes, &proof_bytes, &public_bytes);

    fs::write(dir.join("vk.bin"), &vk_bytes)?;
    fs::write(dir.join("proof.bin"), &proof_bytes)?;
    fs::write(dir.join("public.bin"), &public_bytes)?;

    println!(
        "wrote bls12-381/{slug}/ vk={} B proof={} B public={} B (N={N}, arkworks + zkcrypto both ok)",
        vk_bytes.len(),
        proof_bytes.len(),
        public_bytes.len()
    );

    assert_eq!(vk.gamma_abc_g1.len(), N + 1);
    Ok(())
}

pub fn run(out_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let dir = out_root.join("bls12-381");
    fs::create_dir_all(&dir)?;
    generate_square(&dir)?;
    generate_squares_n::<5>(&dir, "squares-5", 0xB15C_0DE5)?;
    Ok(())
}
