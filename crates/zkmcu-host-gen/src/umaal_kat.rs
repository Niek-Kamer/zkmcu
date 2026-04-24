//! UMAAL Known-Answer Test (KAT) fixture generator.
//!
//! Produces `crates/zkmcu-vectors/data/umaal-kat/kat.bin`: N records of
//! `(a, b, a*b)` where each value is a 32-byte big-endian Fq element
//! (BN254 base field). Host-side computation goes through substrate-bn's
//! pure-Rust `mul_reduce` path; the committed file is the expected output.
//!
//! Why this exists: the fork at `vendor/bn` has a `cortex-m33-asm` feature
//! that swaps `mul_reduce` for hand-written ARMv8-M UMAAL assembly on the
//! Pico 2 W. A silent miscompute in that asm would mean `Ok(true)` on
//! forged proofs, wich is the highest-blast-radius failure mode in the repo. The
//! host differential test (`differential_arkworks.rs`) validates the Rust
//! path against arkworks; this KAT validates the asm path against the Rust
//! path at runtime on real silicon. Firmware flashes that fail the KAT
//! halt before printing any benchmark numbers.
//!
//! Layout: N records, 96 bytes each (3 × 32B BE). No header, no count
//! prefix, length is derivable from the file size.

use std::fs;
use std::path::Path;

use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha20Rng;
use substrate_bn::Fq;

/// Number of random (a, b) pairs. 256 × 96 B = 24 KB on flash, small
/// enough to round-trip through ~300 ms of verify time, exercises enough
/// limb patterns to catch Hamming-weight-specific asm bugs.
const N: usize = 256;
const SEED: u64 = 0xFEED_BEEF_D1FF_0001;
const RECORD_SIZE: usize = 32 * 3;

pub fn run(out_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let dir = out_root.join("umaal-kat");
    fs::create_dir_all(&dir)?;

    let mut rng = ChaCha20Rng::seed_from_u64(SEED);
    let mut out = Vec::with_capacity(N * RECORD_SIZE);
    let mut a_bytes = [0u8; 32];
    let mut b_bytes = [0u8; 32];
    let mut c_bytes = [0u8; 32];
    for _ in 0..N {
        let a = Fq::random(&mut rng);
        let b = Fq::random(&mut rng);
        let c = a * b;
        a.to_big_endian(&mut a_bytes)
            .map_err(|_| "Fq a to_big_endian")?;
        b.to_big_endian(&mut b_bytes)
            .map_err(|_| "Fq b to_big_endian")?;
        c.to_big_endian(&mut c_bytes)
            .map_err(|_| "Fq product to_big_endian")?;
        out.extend_from_slice(&a_bytes);
        out.extend_from_slice(&b_bytes);
        out.extend_from_slice(&c_bytes);
    }
    fs::write(dir.join("kat.bin"), &out)?;

    println!("wrote umaal-kat/kat.bin N={} bytes={}", N, out.len());

    Ok(())
}
