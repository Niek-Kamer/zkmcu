//! Phase-1 measurement: constraint counts and proving key sizes for the
//! Poseidon Merkle membership circuit at depth 3, 4, and 5.
//!
//! Usage:
//!   cargo run -p zkmcu-host-gen --release -- measure-poseidon

// All KB conversions are intentional integer divisions (truncated is fine for
// rough sizing).
#![allow(clippy::integer_division)]

use ark_bn254::{Bn254, Fr};
use ark_groth16::Groth16;
use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystem, SynthesisMode};
use ark_serialize::CanonicalSerialize;
use ark_snark::SNARK;
use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};
use zkmcu_poseidon_circuit::PoseidonMerkleCircuit;

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    println!();
    println!("=== Poseidon Merkle circuit measurement (BN254) ===");
    println!();
    println!(
        "{:<7} {:>12} {:>12} {:>12} {:>12} {:>12} {:>12}",
        "depth", "constraints", "witnesses", "pk_raw KB", "pk_cmp KB", "est_sram KB", "fits?"
    );
    println!("{}", "-".repeat(80));

    for depth in [3usize, 4, 5, 6, 7, 8, 9, 10] {
        let circuit_no_witness = PoseidonMerkleCircuit::<Fr> {
            depth,
            root: None,
            leaf: None,
            siblings: None,
            directions: None,
        };

        // Count constraints and variables in setup mode (closures not called).
        let cs = ConstraintSystem::<Fr>::new_ref();
        cs.set_mode(SynthesisMode::Setup);
        circuit_no_witness
            .clone()
            .generate_constraints(cs.clone())?;
        let n_constraints = cs.num_constraints();
        let n_witnesses = cs.num_witness_variables();
        let n_public = cs.num_instance_variables();
        let n_total = n_witnesses + n_public;

        // Generate proving key and measure serialized size.
        let mut rng = ChaCha20Rng::seed_from_u64(0x00c0_ffee);
        let (pk, _vk) =
            Groth16::<Bn254>::circuit_specific_setup(circuit_no_witness, &mut rng)?;

        // Uncompressed = what the prover holds in RAM.
        let mut pk_raw = Vec::new();
        pk.serialize_uncompressed(&mut pk_raw)?;
        let pk_raw_kb = pk_raw.len() / 1024;

        // Compressed = what you'd store on flash.
        let mut pk_cmp = Vec::new();
        pk.serialize_compressed(&mut pk_cmp)?;
        let pk_cmp_kb = pk_cmp.len() / 1024;

        // Per-query sizes (each G1 affine = 64 B uncompressed, G2 = 128 B).
        let g1_b = 64usize;
        let g2_b = 128usize;
        let a_kb = pk.a_query.len() * g1_b / 1024;
        let b_g1_kb = pk.b_g1_query.len() * g1_b / 1024;
        let b_g2_kb = pk.b_g2_query.len() * g2_b / 1024;
        let h_kb = pk.h_query.len() * g1_b / 1024;
        let l_kb = pk.l_query.len() * g1_b / 1024;

        // Rough in-SRAM estimate: pk_raw + FFT workspace + witness values.
        // FFT domain = next power of 2 above n_constraints.
        let fft_domain = n_constraints.next_power_of_two();
        let fft_kb = fft_domain * 32 / 1024;         // one Fr per domain element
        let witness_kb = n_total * 32 / 1024;         // one Fr per variable
        let est_sram_kb = pk_raw_kb + fft_kb + witness_kb;
        let fits = if est_sram_kb <= 520 { "YES" } else { "NO" };

        println!(
            "{depth:<7} {n_constraints:>12} {n_witnesses:>12} {pk_raw_kb:>12} {pk_cmp_kb:>12} {est_sram_kb:>12} {fits:>12}",
        );

        // Detailed query breakdown.
        println!(
            "         queries: a={a_kb}KB  b_g1={b_g1_kb}KB  b_g2={b_g2_kb}KB  h={h_kb}KB  l={l_kb}KB  \
             total_vars={n_total} (witnesses={n_witnesses} public={n_public})"
        );
        println!(
            "         fft_domain={fft_domain}  fft_est={fft_kb}KB  witness_est={witness_kb}KB",
        );
        println!();
    }

    println!("Pico 2 W SRAM budget: 520 KB total (heap=96KB stack=16KB leaves ~408KB for prover)");
    println!();

    Ok(())
}
