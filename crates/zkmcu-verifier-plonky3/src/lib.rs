//! # zkmcu-verifier-plonky3: STARK verification on microcontrollers (Plonky3 backend)
//!
//! A `no_std` wrapper around [Plonky3]'s `p3-uni-stark::verify`, designed
//! to run on ARM Cortex-M and RISC-V microcontrollers. Sibling crate to
//! [`zkmcu-verifier-stark`] (winterfell backend), with the same shape but
//! a different field / hash story.
//!
//! ## Why two STARK verifiers in the tree
//!
//! `zkmcu-verifier-stark` (winterfell) was built first and underpins the
//! existing Fibonacci / threshold benches on `BabyBear` × Quartic at
//! 95-bit conjectured security. Winterfell's tree does not ship with
//! `BabyBear` or Poseidon2 (only Goldilocks + Blake3 / Rescue), so when
//! the PQ-Semaphore milestone needed the audited Poseidon2-`BabyBear`
//! parameters from `crates/zkmcu-poseidon-audit`, porting them into
//! winterfell would have invalidated the audit's coverage and required
//! a second audit pass. This crate uses Plonky3's verifier instead so
//! the audited code paths run unmodified on-device.
//!
//! See the spike at
//! [`research/notebook/2026-04-29-pq-semaphore-verifier-spike.md`][spike]
//! for the decision rationale.
//!
//! [spike]: https://github.com/Niek-Kamer/zkmcu/blob/main/research/notebook/2026-04-29-pq-semaphore-verifier-spike.md
//!
//! ## Supported AIRs
//!
//! *Phase 4.0 step 2, scaffold only.* No concrete AIRs are wired up yet.
//! Phase 4.0 step 3 will add a minimal Poseidon2 hash-chain AIR as the
//! first bench vector so the verifier wiring gets exercised before the
//! full PQ-Semaphore AIR lands. Phase 4.0 step 4 adds the
//! Merkle-membership + nullifier + scope-binding AIR proper.
//!
//! ## Proof + public-inputs wire format
//!
//! Plonky3's `Proof<SC>` is generic over a [`StarkGenericConfig`] that
//! pins the field, extension, hash, and FRI parameters. Serialization is
//! `serde`-based; on-device parsing therefore needs a `no_std`-compatible
//! serde format. The intended encoding is `postcard` (already vendored
//! for Plonky3's own dev-deps, `default-features = false`). Concrete
//! `parse_proof_<name>` and `verify_<name>` functions land alongside the
//! first AIR.
//!
//! ## Dependencies
//!
//! Pulls only `p3-uni-stark` directly. Its transitive closure brings in
//! `p3-air`, `p3-baby-bear`, `p3-challenger`, `p3-commit`, `p3-field`,
//! `p3-fri`, `p3-matrix`, `p3-merkle-tree`, `p3-poseidon2`, `p3-symmetric`
//! — i.e. exactly the verifier-side stack the spike built. All of them
//! declare `#![no_std]` + `extern crate alloc` and build clean for both
//! `thumbv8m.main-none-eabihf` and `riscv32imac-unknown-none-elf` with
//! zero patches. Validated in the spike crate at
//! `research/notebook/2026-04-29-pq-semaphore-verifier-spike/`.
//!
//! [Plonky3]: https://github.com/Plonky3/Plonky3
//! [`zkmcu-verifier-stark`]: https://crates.io/crates/zkmcu-verifier-stark
//! [`StarkGenericConfig`]: p3_uni_stark::StarkGenericConfig

#![no_std]

extern crate alloc;

pub mod poseidon2_chain;
pub mod pq_semaphore;
pub mod pq_semaphore_blake3;
pub mod pq_semaphore_dual;
pub mod pq_semaphore_goldilocks;

// Re-export the Plonky3 surface downstream consumers need. Anchors the
// public API to this crate so callers don't need a direct dep on
// `p3-uni-stark`. Concrete AIR-specific entry points land as
// sibling modules in subsequent phases.
pub use p3_uni_stark::{verify, Proof, StarkGenericConfig, VerificationError};

/// Upper bound on the byte length accepted by the AIR-specific
/// `parse_proof_<name>` functions added in later phases.
///
/// PQ-Semaphore-BabyBear proof size at 64 FRI queries / 16 rows / d=6
/// is ~ 172 KB. The Goldilocks × Quadratic sibling pays double the per-
/// element bytes (8 vs 4) and lands at ~ 271 KB, so the cap was bumped
/// from 256 → 320 KB to fit. This is a length-prefix attack-surface
/// cap, not a heap budget — firmware passes the static flash slice
/// directly to `parse_proof` so the bytes never hit the heap. The
/// poseidon2-chain anchor AIR (28 queries, 64 rows) fits in ~ 88 KB,
/// well within this cap.
///
/// [`zkmcu-verifier-stark`]: https://crates.io/crates/zkmcu-verifier-stark
pub const MAX_PROOF_SIZE: usize = 320 * 1024;

/// Unified error type spanning parse and verify failures.
///
/// Mirrors the `Error` shape on [`zkmcu-verifier`], [`zkmcu-verifier-bls12`]
/// and [`zkmcu-verifier-stark`] so downstream code can uniformly handle
/// failure across all four verifier families.
///
/// [`zkmcu-verifier`]: https://crates.io/crates/zkmcu-verifier
/// [`zkmcu-verifier-bls12`]: https://crates.io/crates/zkmcu-verifier-bls12
/// [`zkmcu-verifier-stark`]: https://crates.io/crates/zkmcu-verifier-stark
#[derive(Debug)]
pub enum Error {
    /// The serde / postcard decoder rejected the proof bytes. Wrong
    /// length, wrong layout, or bytes outside the field modulus inside
    /// the encoded values.
    ProofDeserialization,
    /// The AIR-specific public-inputs parser rejected the input. Wrong
    /// length, wrong encoding, or values outside the field modulus.
    PublicDeserialization,
    /// Verification itself failed. The [`VerificationError`] inside is
    /// generic over the PCS error type; AIR-specific entry points
    /// erase the parameter to keep this enum's shape stable across AIRs.
    VerificationFailed,
}
