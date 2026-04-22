//! BN254 vector generation (EIP-197 wire format).
//!
//! Produces `crates/zkmcu-vectors/data/{square,squares-5}/{vk,proof,public}.bin`.
//! This is the original host-gen code, extracted so the crate can also
//! generate BLS12-381 vectors under `data/bls12-381/`.

use std::fs;
use std::path::Path;

use ark_bn254::{Bn254, Fq, Fr};
use ark_ec::AffineRepr;
use ark_ff::{BigInteger, PrimeField, Zero};
use ark_groth16::Groth16;
use ark_snark::SNARK;
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha20Rng;

use crate::circuits::{SquareCircuit, SquaresNCircuit};

// ---- EIP-197 serialization ---------------------------------------------

fn fq_to_be32(f: Fq) -> [u8; 32] {
    let bytes = f.into_bigint().to_bytes_be();
    let mut out = [0u8; 32];
    let start = 32_usize.saturating_sub(bytes.len());
    out.get_mut(start..)
        .expect("Fq serialization fits in 32 bytes")
        .copy_from_slice(&bytes);
    out
}

fn fr_to_be32(f: Fr) -> [u8; 32] {
    let bytes = f.into_bigint().to_bytes_be();
    let mut out = [0u8; 32];
    let start = 32_usize.saturating_sub(bytes.len());
    out.get_mut(start..)
        .expect("Fr serialization fits in 32 bytes")
        .copy_from_slice(&bytes);
    out
}

fn g1_to_eip197(p: &ark_bn254::G1Affine) -> [u8; 64] {
    let mut out = [0u8; 64];
    if let Some((x, y)) = p.xy() {
        let (head, tail) = out.split_at_mut(32);
        head.copy_from_slice(&fq_to_be32(x));
        tail.copy_from_slice(&fq_to_be32(y));
    }
    out
}

fn g2_to_eip197(p: &ark_bn254::G2Affine) -> [u8; 128] {
    let mut out = [0u8; 128];
    if let Some((x, y)) = p.xy() {
        let (xs, ys) = out.split_at_mut(64);
        let (x_c1, x_c0) = xs.split_at_mut(32);
        x_c1.copy_from_slice(&fq_to_be32(x.c1));
        x_c0.copy_from_slice(&fq_to_be32(x.c0));
        let (y_c1, y_c0) = ys.split_at_mut(32);
        y_c1.copy_from_slice(&fq_to_be32(y.c1));
        y_c0.copy_from_slice(&fq_to_be32(y.c0));
    }
    out
}

fn u32_len(n: usize) -> [u8; 4] {
    u32::try_from(n)
        .expect("collection length fits in u32 for EIP-197 vk/proof/public")
        .to_le_bytes()
}

fn encode_vk(vk: &ark_groth16::VerifyingKey<Bn254>) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&g1_to_eip197(&vk.alpha_g1));
    out.extend_from_slice(&g2_to_eip197(&vk.beta_g2));
    out.extend_from_slice(&g2_to_eip197(&vk.gamma_g2));
    out.extend_from_slice(&g2_to_eip197(&vk.delta_g2));
    out.extend_from_slice(&u32_len(vk.gamma_abc_g1.len()));
    for ic in &vk.gamma_abc_g1 {
        out.extend_from_slice(&g1_to_eip197(ic));
    }
    out
}

fn encode_proof(proof: &ark_groth16::Proof<Bn254>) -> Vec<u8> {
    let mut out = Vec::with_capacity(256);
    out.extend_from_slice(&g1_to_eip197(&proof.a));
    out.extend_from_slice(&g2_to_eip197(&proof.b));
    out.extend_from_slice(&g1_to_eip197(&proof.c));
    out
}

fn encode_public(public: &[Fr]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + public.len() * 32);
    out.extend_from_slice(&u32_len(public.len()));
    for p in public {
        out.extend_from_slice(&fr_to_be32(*p));
    }
    out
}

// ---- Vector generation -------------------------------------------------

fn generate_square(out_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let dir = out_root.join("square");
    fs::create_dir_all(&dir)?;

    // Deterministic RNG so repeated generation is stable.
    let mut rng = ChaCha20Rng::seed_from_u64(0xA11C_E5E7);

    let x = Fr::from(3u64);
    let y = x * x; // = 9

    let setup_circuit = SquareCircuit::<Fr> { x: None, y: None };
    let (pk, vk) = Groth16::<Bn254>::circuit_specific_setup(setup_circuit, &mut rng)?;

    let proof_circuit = SquareCircuit::<Fr> {
        x: Some(x),
        y: Some(y),
    };
    let proof = Groth16::<Bn254>::prove(&pk, proof_circuit, &mut rng)?;

    // Native verify, to confirm the vector is valid before serializing.
    let native_ok = Groth16::<Bn254>::verify(&vk, &[y], &proof)?;
    assert!(
        native_ok,
        "native Groth16 verify failed (BN254 square) — refusing to write bad vectors"
    );

    fs::write(dir.join("vk.bin"), encode_vk(&vk))?;
    fs::write(dir.join("proof.bin"), encode_proof(&proof))?;
    fs::write(dir.join("public.bin"), encode_public(&[y]))?;

    let vk_bytes = fs::metadata(dir.join("vk.bin"))?.len();
    let proof_bytes = fs::metadata(dir.join("proof.bin"))?.len();
    let public_bytes = fs::metadata(dir.join("public.bin"))?.len();

    println!("wrote bn254/square/ vk={vk_bytes} B proof={proof_bytes} B public={public_bytes} B");

    // Sanity assertions.
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
    let (pk, vk) = Groth16::<Bn254>::circuit_specific_setup(setup_circuit, &mut rng)?;

    let proof_circuit = SquaresNCircuit::<Fr, N> {
        xs: Some(xs),
        ys: Some(ys),
    };
    let proof = Groth16::<Bn254>::prove(&pk, proof_circuit, &mut rng)?;

    let native_ok = Groth16::<Bn254>::verify(&vk, &ys, &proof)?;
    assert!(
        native_ok,
        "native Groth16 verify failed (BN254 {slug}) — refusing to write bad vectors"
    );

    fs::write(dir.join("vk.bin"), encode_vk(&vk))?;
    fs::write(dir.join("proof.bin"), encode_proof(&proof))?;
    fs::write(dir.join("public.bin"), encode_public(&ys))?;

    let vk_bytes = fs::metadata(dir.join("vk.bin"))?.len();
    let proof_bytes = fs::metadata(dir.join("proof.bin"))?.len();
    let public_bytes = fs::metadata(dir.join("public.bin"))?.len();

    println!(
        "wrote bn254/{slug}/ vk={vk_bytes} B proof={proof_bytes} B public={public_bytes} B (N={N})"
    );

    assert_eq!(vk.gamma_abc_g1.len(), N + 1);
    Ok(())
}

pub fn run(out_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    generate_square(out_root)?;
    generate_squares_n::<5>(out_root, "squares-5", 0xA11C_E5E9)?;
    Ok(())
}
