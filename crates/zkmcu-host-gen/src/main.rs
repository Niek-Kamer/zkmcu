//! Generate Groth16/BN254 test vectors in EIP-197 binary format.
//!
//! Writes `vk.bin`, `proof.bin`, `public.bin` into `crates/zkmcu-vectors/data/<name>/`.
//! The circuits here are intentionally trivial — they exist only so the embedded verifier
//! has something real to chew on.

// This binary is a one-shot CLI — printing to stdout is the point of its existence.
#![allow(clippy::print_stdout)]

use std::fs;
use std::path::{Path, PathBuf};

use ark_bn254::{Bn254, Fq, Fr};
use ark_ec::AffineRepr;
use ark_ff::{BigInteger, PrimeField, Zero};
use ark_groth16::Groth16;
use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};
use ark_snark::SNARK;
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha20Rng;

// ---- Circuits -----------------------------------------------------------

#[derive(Clone)]
struct SquareCircuit {
    x: Option<Fr>,
    y: Option<Fr>,
}

impl ConstraintSynthesizer<Fr> for SquareCircuit {
    fn generate_constraints(self, cs: ConstraintSystemRef<Fr>) -> Result<(), SynthesisError> {
        use ark_relations::r1cs::LinearCombination;

        let y_var = cs.new_input_variable(|| self.y.ok_or(SynthesisError::AssignmentMissing))?;
        let x_var = cs.new_witness_variable(|| self.x.ok_or(SynthesisError::AssignmentMissing))?;

        cs.enforce_constraint(
            LinearCombination::from(x_var),
            LinearCombination::from(x_var),
            LinearCombination::from(y_var),
        )?;
        Ok(())
    }
}

/// N independent `x_i * x_i = y_i` constraints — useful for studying how
/// verifier cost scales with the number of public inputs. Each public `y_i`
/// adds one G1 point to the verifying key's `IC` table and one scalar
/// multiplication + point addition to the `vk_x` linear combination during
/// verification.
#[derive(Clone)]
struct SquaresNCircuit<const N: usize> {
    xs: Option<[Fr; N]>,
    ys: Option<[Fr; N]>,
}

impl<const N: usize> ConstraintSynthesizer<Fr> for SquaresNCircuit<N> {
    fn generate_constraints(self, cs: ConstraintSystemRef<Fr>) -> Result<(), SynthesisError> {
        use ark_relations::r1cs::LinearCombination;

        for i in 0..N {
            let y_var = cs.new_input_variable(|| {
                self.ys
                    .as_ref()
                    .and_then(|ys| ys.get(i).copied())
                    .ok_or(SynthesisError::AssignmentMissing)
            })?;
            let x_var = cs.new_witness_variable(|| {
                self.xs
                    .as_ref()
                    .and_then(|xs| xs.get(i).copied())
                    .ok_or(SynthesisError::AssignmentMissing)
            })?;
            cs.enforce_constraint(
                LinearCombination::from(x_var),
                LinearCombination::from(x_var),
                LinearCombination::from(y_var),
            )?;
        }
        Ok(())
    }
}

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

// ---- Entry point -------------------------------------------------------

fn vectors_data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("host-gen crate is nested under crates/")
        .join("zkmcu-vectors")
        .join("data")
}

fn generate_square(out_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let dir = out_root.join("square");
    fs::create_dir_all(&dir)?;

    // Deterministic RNG so repeated generation is stable.
    let mut rng = ChaCha20Rng::seed_from_u64(0xA11C_E5E7);

    let x = Fr::from(3u64);
    let y = x * x; // = 9

    let setup_circuit = SquareCircuit { x: None, y: None };
    let (pk, vk) = Groth16::<Bn254>::circuit_specific_setup(setup_circuit, &mut rng)?;

    let proof_circuit = SquareCircuit {
        x: Some(x),
        y: Some(y),
    };
    let proof = Groth16::<Bn254>::prove(&pk, proof_circuit, &mut rng)?;

    // Native verify to confirm the vector is valid before we serialize it.
    let native_ok = Groth16::<Bn254>::verify(&vk, &[y], &proof)?;
    assert!(
        native_ok,
        "native Groth16 verify failed — refusing to write bad vectors"
    );

    fs::write(dir.join("vk.bin"), encode_vk(&vk))?;
    fs::write(dir.join("proof.bin"), encode_proof(&proof))?;
    fs::write(dir.join("public.bin"), encode_public(&[y]))?;

    let vk_bytes = fs::metadata(dir.join("vk.bin"))?.len();
    let proof_bytes = fs::metadata(dir.join("proof.bin"))?.len();
    let public_bytes = fs::metadata(dir.join("public.bin"))?.len();

    println!(
        "wrote square/ vk={vk_bytes} B proof={proof_bytes} B public={public_bytes} B (native verify: ok)"
    );

    // Sanity assertion: one public input + one, so IC has 2 points.
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

    // Witnesses: a small set of distinct non-zero values.
    let mut xs = [Fr::from(0u64); N];
    for (i, v) in xs.iter_mut().enumerate() {
        *v = Fr::from((i as u64 + 3) * 17 + 1);
    }
    let mut ys = [Fr::from(0u64); N];
    for (y, x) in ys.iter_mut().zip(xs.iter()) {
        *y = *x * *x;
    }

    let setup_circuit = SquaresNCircuit::<N> { xs: None, ys: None };
    let (pk, vk) = Groth16::<Bn254>::circuit_specific_setup(setup_circuit, &mut rng)?;

    let proof_circuit = SquaresNCircuit::<N> {
        xs: Some(xs),
        ys: Some(ys),
    };
    let proof = Groth16::<Bn254>::prove(&pk, proof_circuit, &mut rng)?;

    let native_ok = Groth16::<Bn254>::verify(&vk, &ys, &proof)?;
    assert!(
        native_ok,
        "native Groth16 verify failed for {slug} — refusing to write bad vectors"
    );

    fs::write(dir.join("vk.bin"), encode_vk(&vk))?;
    fs::write(dir.join("proof.bin"), encode_proof(&proof))?;
    fs::write(dir.join("public.bin"), encode_public(&ys))?;

    let vk_bytes = fs::metadata(dir.join("vk.bin"))?.len();
    let proof_bytes = fs::metadata(dir.join("proof.bin"))?.len();
    let public_bytes = fs::metadata(dir.join("public.bin"))?.len();

    println!(
        "wrote {slug}/ vk={vk_bytes} B proof={proof_bytes} B public={public_bytes} B (N={N}, native verify: ok)"
    );

    assert_eq!(vk.gamma_abc_g1.len(), N + 1);
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_root = vectors_data_dir();
    println!("writing to {}", out_root.display());
    generate_square(&out_root)?;
    generate_squares_n::<5>(&out_root, "squares-5", 0xA11C_E5E9)?;
    Ok(())
}
