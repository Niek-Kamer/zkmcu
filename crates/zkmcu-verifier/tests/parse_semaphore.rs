//! Sanity check that the Semaphore-depth-10 VK extracted from the
//! `vendor/semaphore` submodule by `zkmcu-host-gen semaphore` parses
//! cleanly through `zkmcu-verifier`. This is a no-proof test — we only
//! verify the VK decodes on-curve, not that verify runs, because we
//! don't have a proof committed yet (Semaphore proofs require their
//! snarkjs toolchain, see phase-2.7 notes).

#![allow(clippy::unwrap_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;

use zkmcu_verifier::parse_vk;

fn semaphore_vk_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("zkmcu-vectors")
        .join("data")
        .join("semaphore-depth-10")
        .join("vk.bin")
}

#[test]
fn semaphore_depth_10_vk_parses() {
    let path = semaphore_vk_path();
    let bytes = match fs::read(&path) {
        Ok(b) => b,
        Err(_) => {
            // Semaphore VK is regenerated from the submodule via
            //   cargo run -p zkmcu-host-gen --release -- semaphore
            // and not committed to git (data/.gitignore excludes it) —
            // if the file is missing locally, skip the test rather than
            // fail CI.
            eprintln!("skipping: {} not present", path.display());
            return;
        }
    };
    let vk = parse_vk(&bytes).expect("Semaphore depth-10 VK parses");
    // Semaphore nPublic = 4, so gamma_abc_g1 = nPublic + 1 = 5.
    assert_eq!(
        vk.ic.len(),
        5,
        "Semaphore depth-10 VK should have 5 IC points (nPublic=4 + 1)"
    );
    // All 4 "shared" points + delta + 5 IC all decoded without off-curve
    // errors (parse_vk's on-curve check fired for every one of them).
    // Anything more requires a proof, so we stop here.
}
