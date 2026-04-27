//! Poseidon Merkle membership vector generation for BN254.
//!
//! Produces `crates/zkmcu-vectors/data/poseidon-depth-{3,10}/{vk,proof,public}.bin`.
//! Same placeholder parameters as `zkmcu-poseidon-circuit` (t=3, α=5,
//! 8 full rounds, 57 partial, zeroed ARK, circulant [[2,1,1]…] MDS).
//!
//! `IC_size` = 2 (one public input: the Merkle root) regardless of depth, so
//! verify time on the firmware is depth-independent.  Depth 3 (8 leaves) vs
//! depth 10 (1024 leaves) will return the same cycle count on the Pico.

use std::fs;
use std::path::Path;

use ark_bn254::{Bn254, Fr};
use ark_ff::{Field, Zero};
use ark_groth16::Groth16;
use ark_snark::SNARK;
use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};
use zkmcu_poseidon_circuit::PoseidonMerkleCircuit;

use crate::bn254::{encode_proof, encode_public, encode_vk};

// ---- Native Poseidon (mirrors the circuit exactly) ----------------------

const HALF_ROUNDS: usize = 4;
const PARTIAL_ROUNDS: usize = 57;

fn mds(state: [Fr; 3]) -> [Fr; 3] {
    let [s0, s1, s2] = state;
    [s0 + s0 + s1 + s2, s0 + s1 + s1 + s2, s0 + s1 + s2 + s2]
}

fn sbox(x: Fr) -> Fr {
    let x2 = x.square();
    x2.square() * x
}

fn poseidon_perm(mut state: [Fr; 3]) -> [Fr; 3] {
    for _ in 0..HALF_ROUNDS {
        state = state.map(sbox);
        state = mds(state);
    }
    for _ in 0..PARTIAL_ROUNDS {
        state[0] = sbox(state[0]);
        state = mds(state);
    }
    for _ in 0..HALF_ROUNDS {
        state = state.map(sbox);
        state = mds(state);
    }
    state
}

/// Two-to-one hash: mirrors `poseidon_two_to_one` in the circuit.
/// cap is always 0 (domain separation witness); output is state[1].
fn hash(left: Fr, right: Fr) -> Fr {
    poseidon_perm([Fr::zero(), left, right])[1]
}

// ---- Merkle tree helpers ------------------------------------------------

/// Build all levels of a complete binary tree bottom-up.
/// `levels[0]` = leaves, `levels[depth]` = [root].
fn build_tree(leaves: &[Fr]) -> Vec<Vec<Fr>> {
    let mut levels = vec![leaves.to_vec()];
    while levels.last().expect("at least one level").len() > 1 {
        let prev = levels.last().expect("at least one level");
        let next = prev
            .chunks(2)
            .map(|pair| {
                let left = pair.first().copied().expect("chunks(2) gives non-empty slices");
                hash(left, pair.get(1).copied().unwrap_or_else(Fr::zero))
            })
            .collect();
        levels.push(next);
    }
    levels
}

/// Extract the Merkle path for `leaf_idx`.
/// Returns (root, leaf, siblings bottom-to-top, directions: true = leaf is left child).
fn merkle_path(levels: &[Vec<Fr>], leaf_idx: usize) -> (Fr, Fr, Vec<Fr>, Vec<bool>) {
    let depth = levels.len() - 1;
    let root = levels
        .get(depth)
        .and_then(|l| l.first())
        .copied()
        .expect("build_tree root level always exists");
    let leaf = levels
        .first()
        .and_then(|l| l.get(leaf_idx))
        .copied()
        .expect("leaf_idx is within tree leaves");
    let mut siblings = Vec::with_capacity(depth);
    let mut directions = Vec::with_capacity(depth);
    let mut idx = leaf_idx;
    for level_nodes in levels.iter().take(depth) {
        let is_left = idx % 2 == 0;
        let sib_idx = if is_left { idx + 1 } else { idx - 1 };
        siblings.push(level_nodes.get(sib_idx).copied().unwrap_or_else(Fr::zero));
        directions.push(is_left);
        idx /= 2;
    }
    (root, leaf, siblings, directions)
}

// ---- Vector generation --------------------------------------------------

fn generate(
    out_root: &Path,
    depth: usize,
    seed: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let slug = format!("poseidon-depth-{depth}");
    let dir = out_root.join(&slug);
    fs::create_dir_all(&dir)?;

    // Simple deterministic leaves: Fr::from(1), Fr::from(2), …
    let num_leaves = 1usize << depth;
    let leaves: Vec<Fr> = (1..=num_leaves).map(|i| Fr::from(i as u64)).collect();

    // Build tree and extract membership path for leaf 0.
    let levels = build_tree(&leaves);
    let (root, leaf, siblings, directions) = merkle_path(&levels, 0);

    let mut rng = ChaCha20Rng::seed_from_u64(seed);

    // Setup.
    let setup_circuit = PoseidonMerkleCircuit::<Fr> {
        depth,
        root: None,
        leaf: None,
        siblings: None,
        directions: None,
    };
    let (pk, vk) = Groth16::<Bn254>::circuit_specific_setup(setup_circuit, &mut rng)?;

    // Prove.
    let proof_circuit = PoseidonMerkleCircuit::<Fr> {
        depth,
        root: Some(root),
        leaf: Some(leaf),
        siblings: Some(siblings),
        directions: Some(directions),
    };
    let proof = Groth16::<Bn254>::prove(&pk, proof_circuit, &mut rng)?;

    // Native verify — refuse to write vectors that don't check out.
    let ok = Groth16::<Bn254>::verify(&vk, &[root], &proof)?;
    assert!(
        ok,
        "native Groth16 verify failed for {slug}, refusing to write bad vectors"
    );

    fs::write(dir.join("vk.bin"), encode_vk(&vk))?;
    fs::write(dir.join("proof.bin"), encode_proof(&proof))?;
    fs::write(dir.join("public.bin"), encode_public(&[root]))?;

    let vk_b = fs::metadata(dir.join("vk.bin"))?.len();
    let proof_b = fs::metadata(dir.join("proof.bin"))?.len();
    let public_b = fs::metadata(dir.join("public.bin"))?.len();

    println!(
        "wrote bn254/{slug}/  vk={vk_b} B  proof={proof_b} B  public={public_b} B  \
         (depth={depth} leaves={num_leaves} ic_size={})",
        vk.gamma_abc_g1.len()
    );

    assert_eq!(
        vk.gamma_abc_g1.len(),
        2,
        "IC size must be 2 (one public input: root)"
    );

    Ok(())
}

pub fn run(out_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    generate(out_root, 3, 0xB055_1DE0)?;
    generate(out_root, 10, 0xB055_1DEA)?;
    Ok(())
}
