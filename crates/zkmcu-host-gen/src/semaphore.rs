//! Semaphore VK import.
//!
//! Reads `vendor/semaphore/packages/proof/src/verification-keys.json` — a
//! bundle of 32 Groth16/BN254 VKs, one per Merkle tree depth — extracts
//! the VK for a requested depth, converts to EIP-197 wire format, and
//! writes it as `crates/zkmcu-vectors/data/semaphore-depth-<N>/vk.bin`.
//!
//! The Semaphore bundle's quirk: `alpha / beta / gamma / delta` are shared
//! across all 32 depths, only `IC` varies per depth. Each depth has
//! `nPublic + 1 = 5` IC entries (nPublic=4: merkle root, nullifier,
//! external nullifier, signal hash).
//!
//! Writing `proof.bin` and `public.bin` is a separate step — Semaphore
//! proofs have to be produced by their snarkjs toolchain (no self-
//! contained Rust generator because we don't have the proving key or the
//! circom circuit compiled into arkworks constraint form). See the
//! project README phase-2.7 notes.

use std::fs;
use std::path::Path;
use std::str::FromStr;

use ark_bn254::{Bn254, Fq, Fq2, G1Affine, G2Affine};
use ark_groth16::VerifyingKey;
use serde::Deserialize;

use crate::bn254::encode_vk;

// ---- JSON schema (snarkjs / Semaphore bundle) --------------------------

#[derive(Deserialize)]
struct SemaphoreVkBundle {
    protocol: String,
    curve: String,
    #[serde(rename = "nPublic")]
    n_public: u32,
    /// Shared Jacobian G1 point `[x, y, z]` as decimal strings. z == "1"
    /// means the point is already affine.
    vk_alpha_1: [String; 3],
    /// Shared Jacobian G2 point `[[x.c0, x.c1], [y.c0, y.c1], [z.c0, z.c1]]`
    /// in Semaphore/snarkjs coordinate order (c0 first within each Fp2).
    vk_beta_2: [[String; 2]; 3],
    vk_gamma_2: [[String; 2]; 3],
    /// `vk_delta_2[depth_idx]` is the per-depth delta G2 point. Unlike
    /// alpha/beta/gamma, delta is NOT shared — each depth's Phase 2
    /// trusted-setup contribution produces its own delta.
    vk_delta_2: Vec<[[String; 2]; 3]>,
    /// `IC[depth_idx]` is the IC table for `depth = depth_idx + 1`.
    /// Each table is a list of Jacobian G1 points; its length is
    /// `nPublic + 1` = 5.
    #[serde(rename = "IC")]
    ic: Vec<Vec<[String; 3]>>,
}

// ---- Parsing helpers ---------------------------------------------------

fn parse_fq(s: &str) -> Result<Fq, Box<dyn std::error::Error>> {
    Fq::from_str(s).map_err(|()| format!("invalid Fq decimal string: {s}").into())
}

/// Convert a Jacobian `[x, y, z]` decimal-string triple to an affine G1
/// point. Requires z == "1" (Semaphore's VKs are stored in affine form).
fn g1_from_jacobian(coords: &[String; 3]) -> Result<G1Affine, Box<dyn std::error::Error>> {
    let z = coords.get(2).expect("array len 3 by type").as_str();
    if z != "1" {
        return Err(
            format!("G1 Jacobian z != 1 (got {z}); non-affine points not supported").into(),
        );
    }
    let x = parse_fq(&coords[0])?;
    let y = parse_fq(&coords[1])?;
    let point = G1Affine::new_unchecked(x, y);
    if !point.is_on_curve() {
        return Err("G1 point failed curve check after parse".into());
    }
    Ok(point)
}

/// Convert a Jacobian `[[x.c0, x.c1], [y.c0, y.c1], [z.c0, z.c1]]` to affine G2.
/// Snarkjs stores Fp2 as `(c0, c1)` — arkworks' `Fq2::new(c0, c1)` takes
/// the same order, so no swap at this stage. The EIP-197 wire format uses
/// `(c1, c0)` order, but `encode_vk` handles that.
fn g2_from_jacobian(coords: &[[String; 2]; 3]) -> Result<G2Affine, Box<dyn std::error::Error>> {
    let z = &coords[2];
    if z[0] != "1" || z[1] != "0" {
        return Err(format!("G2 Jacobian z != (1, 0); got ({}, {})", z[0], z[1]).into());
    }
    let x_c0 = parse_fq(&coords[0][0])?;
    let x_c1 = parse_fq(&coords[0][1])?;
    let y_c0 = parse_fq(&coords[1][0])?;
    let y_c1 = parse_fq(&coords[1][1])?;
    let x = Fq2::new(x_c0, x_c1);
    let y = Fq2::new(y_c0, y_c1);
    let point = G2Affine::new_unchecked(x, y);
    if !point.is_on_curve() {
        return Err("G2 point failed twist check after parse".into());
    }
    Ok(point)
}

// ---- Public API --------------------------------------------------------

fn bundle_path() -> std::path::PathBuf {
    let workspace_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("host-gen crate nested under crates/")
        .parent()
        .expect("crates/ under workspace root")
        .to_path_buf();
    workspace_root.join("vendor/semaphore/packages/proof/src/verification-keys.json")
}

fn import_depth(out_root: &Path, depth: usize) -> Result<(), Box<dyn std::error::Error>> {
    if !(1..=32).contains(&depth) {
        return Err(format!("Semaphore depth must be 1..=32 (got {depth})").into());
    }

    let path = bundle_path();
    let text = fs::read_to_string(&path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    let bundle: SemaphoreVkBundle =
        serde_json::from_str(&text).map_err(|e| format!("parsing {}: {e}", path.display()))?;

    if bundle.protocol != "groth16" {
        return Err(format!("unexpected protocol: {}", bundle.protocol).into());
    }
    if bundle.curve != "bn128" {
        return Err(format!("unexpected curve: {}", bundle.curve).into());
    }
    if bundle.n_public != 4 {
        return Err(format!(
            "unexpected nPublic: {} (expected 4 for Semaphore)",
            bundle.n_public
        )
        .into());
    }
    if bundle.ic.len() < depth {
        return Err(format!(
            "bundle holds {} depths, can't extract depth {}",
            bundle.ic.len(),
            depth
        )
        .into());
    }

    let depth_idx = depth - 1;
    let delta_entry = bundle.vk_delta_2.get(depth_idx).ok_or_else(|| {
        format!(
            "bundle delta list has {} entries, can't extract depth {}",
            bundle.vk_delta_2.len(),
            depth
        )
    })?;
    let ic_for_depth = bundle.ic.get(depth_idx).ok_or_else(|| {
        format!(
            "bundle IC list has {} entries, can't extract depth {}",
            bundle.ic.len(),
            depth
        )
    })?;

    let alpha_g1 = g1_from_jacobian(&bundle.vk_alpha_1)?;
    let beta_g2 = g2_from_jacobian(&bundle.vk_beta_2)?;
    let gamma_g2 = g2_from_jacobian(&bundle.vk_gamma_2)?;
    let delta_g2 = g2_from_jacobian(delta_entry)?;
    let expected_ic_len = (bundle.n_public + 1) as usize;
    if ic_for_depth.len() != expected_ic_len {
        return Err(format!(
            "depth {} IC has {} entries, expected {} (nPublic + 1)",
            depth,
            ic_for_depth.len(),
            expected_ic_len
        )
        .into());
    }
    let gamma_abc_g1: Vec<G1Affine> = ic_for_depth
        .iter()
        .map(g1_from_jacobian)
        .collect::<Result<Vec<_>, _>>()?;

    let vk = VerifyingKey::<Bn254> {
        alpha_g1,
        beta_g2,
        gamma_g2,
        delta_g2,
        gamma_abc_g1,
    };

    let vk_bytes = encode_vk(&vk);

    let slug = format!("semaphore-depth-{depth}");
    let dir = out_root.join(&slug);
    fs::create_dir_all(&dir)?;
    fs::write(dir.join("vk.bin"), &vk_bytes)?;

    println!(
        "wrote {slug}/vk.bin {} B (Semaphore Groth16/BN254, nPublic={}, ic_size={})",
        vk_bytes.len(),
        bundle.n_public,
        expected_ic_len
    );

    Ok(())
}

pub fn run(out_root: &Path, depths: &[usize]) -> Result<(), Box<dyn std::error::Error>> {
    for d in depths {
        import_depth(out_root, *d)?;
    }
    Ok(())
}
