//! Generate Groth16 test vectors in EIP-197 (BN254) and EIP-2537 (BLS12-381)
//! binary wire formats. Writes to `crates/zkmcu-vectors/data/`.
//!
//! Usage:
//!   cargo run -p zkmcu-host-gen --release                           # synthetic BN254 + BLS12
//!   cargo run -p zkmcu-host-gen --release -- bn254                  # BN254 synthetic only
//!   cargo run -p zkmcu-host-gen --release -- bls12-381              # BLS12 synthetic only
//!   cargo run -p zkmcu-host-gen --release -- semaphore              # Semaphore depth-10 VK import
//!   cargo run -p zkmcu-host-gen --release -- semaphore --depth 1    # pick a specific depth

// This binary is a one-shot CLI — printing to stdout is the point of its existence.
#![allow(clippy::print_stdout)]
// Bin-crate internal modules talk only to each other; pub items here don't need
// to be "reachable" from outside the crate in the lint's sense.
#![allow(unreachable_pub)]

use std::path::PathBuf;

mod bls12_381;
pub(crate) mod bn254;
mod circuits;
mod semaphore;

fn vectors_data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("host-gen crate is nested under crates/")
        .join("zkmcu-vectors")
        .join("data")
}

/// Parse `--depth N` out of argv. Defaults to 10 (matches Semaphore's own
/// `packages/proof/tests/index.test.ts`, so our first measurement is
/// directly comparable against the Semaphore reference).
fn parse_depth(args: &[String]) -> Result<usize, Box<dyn std::error::Error>> {
    let mut depth = 10usize;
    let mut iter = args.iter();
    while let Some(a) = iter.next() {
        if a == "--depth" {
            let v = iter
                .next()
                .ok_or("--depth needs a value between 1 and 32")?;
            depth = v
                .parse::<usize>()
                .map_err(|e| format!("--depth {v} is not a number: {e}"))?;
        }
    }
    Ok(depth)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out_root = vectors_data_dir();
    println!("writing to {}", out_root.display());

    // Subcommand-ish: if any explicit command name is given, run only that;
    // if the arg list is empty, run the two synthetic generators (back-compat
    // with `just regen-vectors`). Semaphore import is opt-in because it
    // depends on the vendor/semaphore submodule being cloned.
    let has_arg = |name: &str| args.iter().any(|a| a == name);
    let any_explicit =
        has_arg("bn254") || has_arg("bls12-381") || has_arg("bls12_381") || has_arg("semaphore");

    if !any_explicit || has_arg("bn254") {
        bn254::run(&out_root)?;
    }
    if !any_explicit || has_arg("bls12-381") || has_arg("bls12_381") {
        bls12_381::run(&out_root)?;
    }
    if has_arg("semaphore") {
        let depth = parse_depth(&args)?;
        semaphore::run(&out_root, &[depth])?;
    }

    Ok(())
}
