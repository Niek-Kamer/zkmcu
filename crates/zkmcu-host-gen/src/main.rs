//! Generate Groth16 test vectors in EIP-197 (BN254) and EIP-2537 (BLS12-381)
//! binary wire formats. Writes to `crates/zkmcu-vectors/data/`.
//!
//! Usage:
//!   cargo run -p zkmcu-host-gen --release                 # both curves
//!   cargo run -p zkmcu-host-gen --release -- bn254        # BN254 only
//!   cargo run -p zkmcu-host-gen --release -- bls12-381    # BLS12-381 only

// This binary is a one-shot CLI — printing to stdout is the point of its existence.
#![allow(clippy::print_stdout)]
// Bin-crate internal modules talk only to each other; pub items here don't need
// to be "reachable" from outside the crate in the lint's sense.
#![allow(unreachable_pub)]

use std::path::PathBuf;

mod bls12_381;
mod bn254;
mod circuits;

fn vectors_data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("host-gen crate is nested under crates/")
        .join("zkmcu-vectors")
        .join("data")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out_root = vectors_data_dir();
    println!("writing to {}", out_root.display());

    let run_all = args.is_empty();
    let run_bn254 = run_all || args.iter().any(|a| a == "bn254");
    let run_bls12 = run_all || args.iter().any(|a| a == "bls12-381" || a == "bls12_381");

    if run_bn254 {
        bn254::run(&out_root)?;
    }
    if run_bls12 {
        bls12_381::run(&out_root)?;
    }

    Ok(())
}
