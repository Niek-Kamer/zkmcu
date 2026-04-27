//! # zkmcu-verifier-stark: STARK verification on microcontrollers
//!
//! A `no_std` wrapper around [winterfell]'s verifier, designed to run on
//! ARM Cortex-M and RISC-V microcontrollers. Sibling crate to
//! [`zkmcu-verifier`] (Groth16 / BN254) and [`zkmcu-verifier-bls12`]
//! (Groth16 / BLS12-381).
//!
//! Unlike the pairing-based siblings, STARK verification has no separate
//! "verifying key" byte buffer. The AIR (Algebraic Intermediate
//! Representation) definition IS the verifier-side invariant, compiled
//! into the firmware. This crate therefore exposes an AIR-specific
//! `verify_<name>(proof_bytes, public_bytes)` function per circuit
//! it supports, rather than the `parse_vk / parse_proof / parse_public /
//! verify` shape the pairing-based crates use.
//!
//! ## Supported AIRs
//!
//! *Phase 3.1.1, scaffold only.* No concrete AIRs are wired up yet.
//! Phase 3.1.2 will add `fibonacci` (the winterfell hello-world) as the
//! first bench vector. Subsequent phases may add hash-chain, range-proof,
//! or a small RISC-V VM segment.
//!
//! ## Proof + public-inputs wire format
//!
//! STARK proofs use winterfell's own binary encoding ([`Proof::to_bytes`]
//! / [`Proof::from_bytes`]). Unlike Groth16 proofs (which are fixed 256 B
//! or 512 B regardless of circuit), STARK proofs are variable-size:
//! typical Fibonacci proofs at 96-bit security land around 40–80 KB.
//! Firmware consumers should size their heap arena accordingly, the
//! [prediction report] budgets 120–180 KB total RAM for STARK verify.
//!
//! Public inputs are also AIR-specific, they're a struct implementing
//! [`winterfell::math::ToElements`], serialised by the host-side prover.
//! This crate will expose an AIR-specific `parse_public_<name>` for each
//! wired-up AIR.
//!
//! ## Dependencies
//!
//! Depends on the [`winterfell`] umbrella crate (version 0.13). The
//! umbrella drags `winter-prover` into the build graph even for verify-
//! only firmware; LTO strips unused prover code at link time. If
//! firmware `.text` ever grows beyond expected, swapping to direct
//! `winter-verifier` + `winter-fri` + `winter-crypto` + `winter-math`
//! sub-deps would avoid the prover entirely. Trade is more moving parts
//! in `Cargo.toml`, so we start with the umbrella and measure first.
//!
//! Dep-fit confirmed in `research/notebook/2026-04-23-stark-prior-art.md`
//! Both `thumbv8m.main-none-eabihf` and `riscv32imac-unknown-none-elf`
//! build clean with `default-features = false` and zero warnings.
//!
//! [winterfell]: https://crates.io/crates/winterfell
//! [`zkmcu-verifier`]: https://crates.io/crates/zkmcu-verifier
//! [`zkmcu-verifier-bls12`]: https://crates.io/crates/zkmcu-verifier-bls12
//! [prediction report]: https://github.com/Niek-Kamer/zkmcu/blob/main/research/reports/2026-04-23-stark-prediction.typ
//! [`Proof::to_bytes`]: winterfell::Proof::to_bytes
//! [`Proof::from_bytes`]: winterfell::Proof::from_bytes

#![no_std]

extern crate alloc;

pub mod fibonacci;
pub mod fibonacci_babybear;
pub mod threshold_check;

// Re-export the winterfell types downstream consumers need without forcing
// them to take a direct dep on winterfell. Keeps the public API surface
// anchored to this crate; swap to direct sub-deps later without breaking
// callers.
pub use winterfell::{AcceptableOptions, Proof, VerifierError};

/// Upper bound on the byte length accepted by [`parse_proof`].
///
/// Real Fibonacci-1024 proofs at 95-bit conjectured security are ~30 KB;
/// 128 KB leaves headroom for larger AIRs while capping the attack
/// surface. **Partial mitigation only**, a well-crafted proof within
/// this size can still drive winterfell's deserializer into an unbounded
/// `Vec::with_capacity` via an adversary-controlled length prefix (see
/// `research/postmortems/2026-04-24-stark-unbounded-vec-alloc.typ`). The real
/// fix is upstream in `winter-utils::read_many` or a deeper
/// pre-validation pass in this crate. Until either lands, this cap
/// bounds *how much work* an attacker can force the parser to do before
/// the allocator panics, which is strictly better than unbounded.
pub const MAX_PROOF_SIZE: usize = 128 * 1024;

/// Unified error type spanning parse and verify failures.
///
/// Mirrors the `Error` shape on [`zkmcu_verifier`] and [`zkmcu_verifier_bls12`]
/// so downstream code can uniformly handle failure across all three
/// verifier families.
///
/// [`zkmcu_verifier`]: https://crates.io/crates/zkmcu-verifier
/// [`zkmcu_verifier_bls12`]: https://crates.io/crates/zkmcu-verifier-bls12
#[derive(Debug)]
pub enum Error {
    /// `Proof::from_bytes` rejected the input. The winterfell error
    /// (a [`winterfell::DeserializationError`]) carries the specific cause;
    /// this variant erases it to keep the public error surface stable
    /// against upstream shape changes.
    ProofDeserialization,
    /// The AIR-specific public-inputs parser rejected the input. Wrong
    /// length, wrong encoding, or values outside the field modulus.
    PublicDeserialization,
    /// The verification itself failed. The inner [`VerifierError`] carries
    /// the specific cause (FRI check failure, constraint violation,
    /// out-of-domain mismatch, etc.).
    Verification(VerifierError),
}

impl From<VerifierError> for Error {
    fn from(value: VerifierError) -> Self {
        Self::Verification(value)
    }
}

/// Pre-validate the first bytes of a winterfell-encoded proof before
/// handing it to the upstream deserializer. Catches the two `new_multi_\
/// segment` assertions that winterfell's `TraceInfo::read_from` does not
/// screen. Either would halt the firmware when the adversary-controlled
/// bytes hit the assert (see `research/postmortems/2026-04-24-stark-cross-\
/// field-panic.typ`).
///
/// Layout reference: `vendor/winterfell/air/src/air/trace_info.rs:272`
/// (the first four bytes of the proof are main width, aux width, aux rands,
/// trace-length log2). This does not validate the rest of the header, the
/// other invariants that `new_multi_segment` checks are already caught by
/// the deserializer itself.
fn sanity_check_proof_header(bytes: &[u8]) -> Result<(), Error> {
    // Truncation at bytes 0..4 is also handled by the upstream deserializer,
    // but checking here keeps the error path panic-free on the shortest
    // inputs too.
    let main = *bytes.first().ok_or(Error::ProofDeserialization)?;
    let aux = *bytes.get(1).ok_or(Error::ProofDeserialization)?;
    let rands = *bytes.get(2).ok_or(Error::ProofDeserialization)?;
    let log2_len = *bytes.get(3).ok_or(Error::ProofDeserialization)?;

    // `main_segment_width > 0`, the upstream deserializer already rejects
    // zero, but we re-check so the error path is visibly ours.
    if main == 0 {
        return Err(Error::ProofDeserialization);
    }

    // `aux == 0 → rands == 0`. The upstream deserializer only checks the
    // other direction (aux != 0 → rands != 0) and lets this case fall
    // through into `new_multi_segment` which asserts on it.
    if aux == 0 && rands != 0 {
        return Err(Error::ProofDeserialization);
    }

    // `trace_length_log2` in [3, 62]. Lower bound mirrors winterfell's own
    // MIN_TRACE_LENGTH (2^3 = 8). Upper bound is our guard against the
    // `2_usize.pow(log2_len)` overflow that wraps to 0 on 64-bit usize for
    // `log2_len >= 64` and halts the subsequent `trace_length >= 8` assert.
    // 62 is comfortably below the wrap point and far beyond any sane trace.
    if !(3..=62).contains(&log2_len) {
        return Err(Error::ProofDeserialization);
    }

    Ok(())
}

/// Parse a winterfell proof from raw bytes. AIR-agnostic: the binary
/// encoding of a `Proof` is independent of the AIR it was generated
/// against (the AIR is applied at verify time, not at parse time).
///
/// # Errors
///
/// Returns [`Error::ProofDeserialization`] if the bytes are malformed,
/// truncated, or contain trailing bytes after a structurally complete
/// `Proof`. Does not validate the proof's correctness, that happens
/// inside [`winterfell::verify`] when an AIR is supplied.
pub fn parse_proof(bytes: &[u8]) -> Result<Proof, Error> {
    use winter_utils::{ByteReader, Deserializable, SliceReader};

    // Cap input size before handing to winterfell. Partial mitigation for
    // the unbounded `Vec::with_capacity` DoS path inside
    // `Queries::read_from` (see MAX_PROOF_SIZE doc-comment and finding).
    // Real proofs sit around 30 KB; 128 KB is generous headroom.
    if bytes.len() > MAX_PROOF_SIZE {
        return Err(Error::ProofDeserialization);
    }
    sanity_check_proof_header(bytes)?;
    let mut reader = SliceReader::new(bytes);
    let proof = Proof::read_from(&mut reader).map_err(|_| Error::ProofDeserialization)?;
    // Winterfell's `Proof::from_bytes` documents that trailing bytes are
    // tolerated. We reject them here so two different byte sequences cannot
    // parse to the same `Proof`. Closes the same malleability class the
    // Groth16 sibling crates close for their fixed-size proofs.
    if reader.has_more_bytes() {
        return Err(Error::ProofDeserialization);
    }
    Ok(proof)
}
