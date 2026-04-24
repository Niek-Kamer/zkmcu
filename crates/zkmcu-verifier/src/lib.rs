//! # zkmcu-verifier: Groth16 / BN254 on microcontrollers
//!
//! A `no_std` Rust verifier for [Groth16] zk-SNARK proofs over the BN254
//! pairing-friendly curve, designed to run on ARM Cortex-M and RISC-V
//! microcontrollers. The crypto backend is [`substrate-bn`]; this crate
//! adds a defensive EIP-197 parser, a [`verify`] call that wraps
//! `pairing_batch`, and a one-shot [`verify_bytes`] entry point.
//!
//! ## When to use this crate
//!
//! - You need to verify a SNARK proof on a device that doesn't run Linux.
//! - You're happy with ~1 second of verification time on a Cortex-M33 at
//!   150 MHz, ~110 KB of RAM, ~75 KB of flash.
//! - You want a known-good cross-check against `arkworks`-generated proofs.
//!
//! If you want to *generate* proofs on-device, you want a different crate;
//! this one is verify-only. Constant-time verification is not enforced:
//! `substrate-bn` is not CT and verify duration varies with public-input
//! Hamming weight. Acceptable when the public inputs and proof are already
//! public; not acceptable if secret data flows into the verify path. See
//! `SECURITY.md` in the repository root for the full threat model.
//!
//! ## Wire format (EIP-197-compatible)
//!
//! - `Fq` / `Fr`: 32-byte big-endian integer, strictly less than the modulus.
//! - `G1`: 64 bytes, `x ‖ y`. The pair `(0, 0)` is the canonical identity.
//! - `G2`: 128 bytes, `x.c1 ‖ x.c0 ‖ y.c1 ‖ y.c0`. Same identity convention.
//! - Verifying key: `alpha(G1) ‖ beta(G2) ‖ gamma(G2) ‖ delta(G2) ‖
//!   num_ic(u32 LE) ‖ ic[num_ic](G1)`.
//! - Proof: `A(G1) ‖ B(G2) ‖ C(G1)`, always 256 bytes.
//! - Public inputs: `count(u32 LE) ‖ input[count](Fr)`.
//!
//! ## Typical use
//!
//! ```no_run
//! # fn load(_: &str) -> Vec<u8> { Vec::new() }
//! // One-shot: single call, easier for most callers.
//! let ok = zkmcu_verifier::verify_bytes(
//!     &load("vk.bin"),
//!     &load("proof.bin"),
//!     &load("public.bin"),
//! ).expect("parsed ok");
//! assert!(ok);
//!
//! // Reuse: parse vk once, verify many proofs against it.
//! let vk = zkmcu_verifier::parse_vk(&load("vk.bin")).expect("parse");
//! for (proof_bytes, public_bytes) in [(load("p1.bin"), load("u1.bin"))] {
//!     let proof  = zkmcu_verifier::parse_proof(&proof_bytes).expect("parse");
//!     let public = zkmcu_verifier::parse_public(&public_bytes).expect("parse");
//!     let _ok = zkmcu_verifier::verify(&vk, &proof, &public).expect("verify");
//! }
//! ```
//!
//! ## Security
//!
//! The parsers are defensive: unbounded `Vec::with_capacity` calls are guarded
//! against malicious `num_ic` / `count` fields, and Fr inputs ≥ scalar modulus
//! are rejected (preventing non-canonical encoding malleability). See
//! `SECURITY.md` in the repository root for the full threat model, what's
//! tested, and what's explicitly out of scope.
//!
//! [Groth16]: https://eprint.iacr.org/2016/260
//! [`substrate-bn`]: https://crates.io/crates/substrate-bn

#![no_std]

extern crate alloc;

use alloc::vec::Vec;

use bn::{pairing_batch, AffineG1, AffineG2, Fq, Fq2, Group, GroupError, Gt};
use substrate_bn as bn;

pub use bn::{Fr, G1, G2};

/// Serialised size of a `G1` point in EIP-197 wire format (`x ‖ y`, 32 bytes each).
pub const G1_SIZE: usize = 64;
/// Serialised size of a `G2` point in EIP-197 wire format (four 32-byte Fq coordinates).
pub const G2_SIZE: usize = 128;
/// Serialised size of an `Fr` scalar, big-endian, strictly less than the scalar modulus.
pub const FR_SIZE: usize = 32;
/// Serialised size of a Groth16 proof (`A ‖ B ‖ C`). Always 256 bytes.
pub const PROOF_SIZE: usize = G1_SIZE + G2_SIZE + G1_SIZE;

/// Maximum accepted `num_ic` in a verifying key (size of `gamma_abc_g1`).
///
/// Real Groth16 circuits use single-digit to low-hundreds `num_ic` entries;
/// 1024 is a generous ceiling that still prevents pathological VKs with
/// millions of claimed entries from fitting in memory and draining compute.
pub const MAX_NUM_IC: usize = 1024;

/// Maximum accepted public-input count.
///
/// Always exactly `MAX_NUM_IC - 1`, since `num_ic = public_inputs + 1` for any
/// well-formed Groth16 VK. Kept as a separate constant for API clarity at the
/// `parse_public` boundary.
pub const MAX_PUBLIC_INPUTS: usize = MAX_NUM_IC - 1;

/// Anything the parser / verifier can fail with.
///
/// All variants are recoverable, no panic path ever returns an `Error`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// The input buffer length does not match the wire format: too short for
    /// the declared structure, or longer (trailing bytes that would introduce
    /// malleability if callers ever use the raw bytes as a semantic identity).
    TruncatedInput,
    /// An `Fq` base-field element was ≥ the base modulus, or otherwise malformed.
    InvalidFq,
    /// An `Fr` scalar-field element was ≥ the scalar modulus (non-canonical
    /// encoding, rejected to preserve identity semantics).
    InvalidFr,
    /// A `G1` point's `(x, y)` did not satisfy the curve equation.
    InvalidG1,
    /// A `G2` point's `(x, y)` did not satisfy the twist equation.
    InvalidG2Curve,
    /// A `G2` point satisfied the twist equation but is not in the order-`r`
    /// subgroup. Historically a bug class in BN254 precompile implementations;
    /// an adversary who can plant a non-subgroup point in `proof.B` can break
    /// Groth16 soundness if the verifier skips this check.
    InvalidG2Subgroup,
    /// `public.len() + 1 != vk.ic.len()`, the number of public inputs does
    /// not match the VK's `gamma_abc_g1` table size.
    PublicInputCount,
    /// A VK declared `num_ic` above [`MAX_NUM_IC`], or a public-inputs buffer
    /// declared `count` above [`MAX_PUBLIC_INPUTS`]. Real Groth16 circuits use
    /// single-digit to low-hundreds entries; the cap prevents pathologically
    /// large inputs that fit in memory but would cause degenerate verify work.
    InputLimitExceeded,
}

/// A parsed Groth16 verifying key. Built by [`parse_vk`].
///
/// `ic[0] + Σ public[i] * ic[i+1]` gives the `vk_x` point used by the
/// final pairing check. `ic.len()` is always `public_inputs + 1`.
#[derive(Debug, Clone)]
pub struct VerifyingKey {
    /// `α` in G1.
    pub alpha: G1,
    /// `β` in G2.
    pub beta: G2,
    /// `γ` in G2.
    pub gamma: G2,
    /// `δ` in G2.
    pub delta: G2,
    /// `gamma_abc_g1`, one entry per public input plus a leading constant term.
    pub ic: Vec<G1>,
}

/// A parsed Groth16 proof. Built by [`parse_proof`].
#[derive(Debug, Clone)]
pub struct Proof {
    /// `A` in G1.
    pub a: G1,
    /// `B` in G2.
    pub b: G2,
    /// `C` in G1.
    pub c: G1,
}

/// Single-call convenience: parse three byte buffers and run verification.
///
/// Use this when you only verify a given `(vk, proof, public)` triple once.
/// If you verify the same `vk` against many proofs, parse it once with
/// [`parse_vk`] and call [`verify`] repeatedly instead.
///
/// # Errors
///
/// Returns [`Error::TruncatedInput`] if any buffer is shorter than the wire
/// format requires, [`Error::InvalidFq`] / [`Error::InvalidFr`] /
/// [`Error::InvalidG1`] / [`Error::InvalidG2Curve`] / [`Error::InvalidG2Subgroup`]
/// for malformed field elements or off-curve / off-subgroup points,
/// [`Error::InputLimitExceeded`] if the VK or public-input count exceeds the
/// implementation cap, and [`Error::PublicInputCount`] if the number of public
/// inputs doesn't match the VK's `gamma_abc_g1` table.
///
/// # Example
///
/// ```no_run
/// # fn load(_: &str) -> Vec<u8> { Vec::new() }
/// let vk_bytes = load("vk.bin");
/// let proof_bytes = load("proof.bin");
/// let public_bytes = load("public.bin");
/// match zkmcu_verifier::verify_bytes(&vk_bytes, &proof_bytes, &public_bytes) {
///     Ok(true)  => println!("proof valid"),
///     Ok(false) => println!("proof invalid"),
///     Err(e)    => println!("malformed input: {e:?}"),
/// }
/// ```
pub fn verify_bytes(vk: &[u8], proof: &[u8], public: &[u8]) -> Result<bool, Error> {
    let vk = parse_vk(vk)?;
    let proof = parse_proof(proof)?;
    let public = parse_public(public)?;
    verify(&vk, &proof, &public)
}

/// Verify a Groth16 proof against a verifying key and public inputs.
///
/// Checks that `e(-A, B) · e(α, β) · e(vk_x, γ) · e(C, δ)` equals the
/// identity in the target group `Gt`, where `vk_x = ic[0] + Σ input[i] * ic[i+1]`.
/// Internally this is evaluated as a single `pairing_batch` call, the four
/// Miller loops share the expensive final exponentiation.
///
/// Use this when you want to verify many proofs against the same VK. For
/// one-shot verification from raw bytes, see [`verify_bytes`].
///
/// # Errors
///
/// Returns [`Error::PublicInputCount`] if `public.len() + 1 != vk.ic.len()`.
/// This function does not otherwise fail, all validation of byte inputs
/// happens during parsing.
pub fn verify(vk: &VerifyingKey, proof: &Proof, public: &[Fr]) -> Result<bool, Error> {
    if public.len() + 1 != vk.ic.len() {
        return Err(Error::PublicInputCount);
    }

    let (ic_head, ic_tail) = vk.ic.split_first().ok_or(Error::PublicInputCount)?;
    let mut vk_x = *ic_head;
    for (input, ic_i) in public.iter().zip(ic_tail.iter()) {
        vk_x = vk_x + (*ic_i * *input);
    }

    let result = pairing_batch(&[
        (-proof.a, proof.b),
        (vk.alpha, vk.beta),
        (vk_x, vk.gamma),
        (proof.c, vk.delta),
    ]);

    Ok(result == Gt::one())
}

// ---- EIP-197 parsing ----------------------------------------------------

/// Read an exact-size chunk starting at `offset` with no panic on short input.
fn chunk<const N: usize>(bytes: &[u8], offset: usize) -> Result<&[u8; N], Error> {
    bytes
        .get(offset..)
        .and_then(<[u8]>::first_chunk::<N>)
        .ok_or(Error::TruncatedInput)
}

/// Parse a 32-byte big-endian base-field element.
fn read_fq(bytes: &[u8], offset: usize) -> Result<Fq, Error> {
    let c = chunk::<32>(bytes, offset)?;
    Fq::from_slice(c).map_err(|_| Error::InvalidFq)
}

/// BN254 scalar-field modulus `r`, big-endian:
/// `0x30644e72e131a029b85045b68181585d2833e84879b9709143e1f593f0000001`.
///
/// `substrate-bn`'s `Fr::from_slice` silently reduces any 256-bit value mod `r`,
/// which allows non-canonical encodings of public inputs. That's a malleability
/// issue for any application that uses Fr bytes as a semantic identity
/// (nullifiers, replay-protection tags, merkle leaves). We enforce strict
/// canonical encoding, values strictly less than `r`.
const FR_MODULUS_BE: [u8; 32] = [
    0x30, 0x64, 0x4e, 0x72, 0xe1, 0x31, 0xa0, 0x29, 0xb8, 0x50, 0x45, 0xb6, 0x81, 0x81, 0x58, 0x5d,
    0x28, 0x33, 0xe8, 0x48, 0x79, 0xb9, 0x70, 0x91, 0x43, 0xe1, 0xf5, 0x93, 0xf0, 0x00, 0x00, 0x01,
];

/// Parse a 32-byte big-endian scalar at a given offset. Rejects non-canonical
/// encodings (values ≥ BN254 scalar modulus).
pub fn read_fr_at(bytes: &[u8], offset: usize) -> Result<Fr, Error> {
    let c = chunk::<FR_SIZE>(bytes, offset)?;
    if c.as_slice() >= FR_MODULUS_BE.as_slice() {
        return Err(Error::InvalidFr);
    }
    Fr::from_slice(c).map_err(|_| Error::InvalidFr)
}

/// Parse a 64-byte G1 point starting at the given offset (x ‖ y, big-endian each).
pub fn read_g1(bytes: &[u8], offset: usize) -> Result<G1, Error> {
    let x = read_fq(bytes, offset)?;
    let y = read_fq(bytes, offset + 32)?;

    // (0, 0) is the canonical identity encoding in EIP-197.
    if x.is_zero() && y.is_zero() {
        return Ok(G1::zero());
    }

    let affine = AffineG1::new(x, y).map_err(|_| Error::InvalidG1)?;
    Ok(G1::from(affine))
}

/// Parse a 128-byte G2 point starting at the given offset
/// (x.c1 ‖ x.c0 ‖ y.c1 ‖ y.c0, big-endian each).
pub fn read_g2(bytes: &[u8], offset: usize) -> Result<G2, Error> {
    let x_c1 = read_fq(bytes, offset)?;
    let x_c0 = read_fq(bytes, offset + 32)?;
    let y_c1 = read_fq(bytes, offset + 64)?;
    let y_c0 = read_fq(bytes, offset + 96)?;

    let x = Fq2::new(x_c0, x_c1);
    let y = Fq2::new(y_c0, y_c1);

    if x.is_zero() && y.is_zero() {
        return Ok(G2::zero());
    }

    // `AffineG2::new` enforces both the twist equation *and* the order-r
    // subgroup check (substrate-bn sets `G2Params::check_order() = true`). We
    // distinguish the two failure modes so callers can observe which defense
    // fired and tests can pin the behaviour, a silent regression in the
    // upstream subgroup check would otherwise be invisible here.
    let affine = AffineG2::new(x, y).map_err(|e| match e {
        GroupError::NotOnCurve => Error::InvalidG2Curve,
        GroupError::NotInSubgroup => Error::InvalidG2Subgroup,
    })?;
    Ok(G2::from(affine))
}

// ---- Container parsing --------------------------------------------------

fn read_u32_le(bytes: &[u8], offset: usize) -> Result<u32, Error> {
    let c = chunk::<4>(bytes, offset)?;
    Ok(u32::from_le_bytes(*c))
}

/// Parse the verifying-key container.
pub fn parse_vk(bytes: &[u8]) -> Result<VerifyingKey, Error> {
    const HEADER: usize = G1_SIZE + 3 * G2_SIZE;

    let alpha = read_g1(bytes, 0)?;
    let beta = read_g2(bytes, G1_SIZE)?;
    let gamma = read_g2(bytes, G1_SIZE + G2_SIZE)?;
    let delta = read_g2(bytes, G1_SIZE + 2 * G2_SIZE)?;

    let num_ic = read_u32_le(bytes, HEADER)? as usize;
    let ic_start = HEADER + 4;

    // Defence in depth: even if a VK declares a num_ic that *would* fit in a
    // pathologically large buffer, reject it. Real circuits stay well under
    // MAX_NUM_IC (Semaphore is 5, BN254 precompile-scale VKs are ≤100).
    if num_ic > MAX_NUM_IC {
        return Err(Error::InputLimitExceeded);
    }

    // Validate num_ic against actual buffer length *before* allocating.
    // Without this an attacker sends num_ic = u32::MAX and we allocate
    // u32::MAX * G1_SIZE = ~412 GB, instant DoS / OOM. Use checked
    // arithmetic because on 32-bit targets (MCU) the product may overflow.
    let ic_bytes = num_ic.checked_mul(G1_SIZE).ok_or(Error::TruncatedInput)?;
    let ic_end = ic_start
        .checked_add(ic_bytes)
        .ok_or(Error::TruncatedInput)?;
    // Strict length: reject both underflow and trailing bytes. Pinned by
    // `parse_vk_rejects_trailing_byte`; malleability guard for any caller
    // that hashes VK bytes as a semantic identity.
    if bytes.len() != ic_end {
        return Err(Error::TruncatedInput);
    }

    let mut ic = Vec::with_capacity(num_ic);
    for i in 0..num_ic {
        ic.push(read_g1(bytes, ic_start + i * G1_SIZE)?);
    }

    Ok(VerifyingKey {
        alpha,
        beta,
        gamma,
        delta,
        ic,
    })
}

/// Parse an EIP-197-format proof buffer.
///
/// The buffer must be at least [`PROOF_SIZE`] (256) bytes. Layout:
/// `A(G1) ‖ B(G2) ‖ C(G1)`.
///
/// # Errors
///
/// Same shape as [`parse_vk`]: truncation / malformed field element / off-curve point.
pub fn parse_proof(bytes: &[u8]) -> Result<Proof, Error> {
    // Strict length. A Groth16 proof is exactly [`PROOF_SIZE`] bytes; any
    // divergence is a wire-format violation. Rejecting trailing bytes
    // closes the malleability class where two different inputs parse to
    // the same `Proof`.
    if bytes.len() != PROOF_SIZE {
        return Err(Error::TruncatedInput);
    }
    let a = read_g1(bytes, 0)?;
    let b = read_g2(bytes, G1_SIZE)?;
    let c = read_g1(bytes, G1_SIZE + G2_SIZE)?;
    Ok(Proof { a, b, c })
}

/// Parse a public-inputs buffer: `count(u32 LE) ‖ input[count](Fr)`.
///
/// Like [`parse_vk`] this validates the adversary-controlled `count` field
/// against the real buffer length before allocating.
///
/// # Errors
///
/// Returns [`Error::TruncatedInput`] if the buffer is too short for the
/// claimed `count`, and [`Error::InvalidFr`] if any scalar is ≥ the scalar
/// modulus `r`.
pub fn parse_public(bytes: &[u8]) -> Result<Vec<Fr>, Error> {
    let count = read_u32_le(bytes, 0)? as usize;

    if count > MAX_PUBLIC_INPUTS {
        return Err(Error::InputLimitExceeded);
    }

    let inputs_bytes = count.checked_mul(FR_SIZE).ok_or(Error::TruncatedInput)?;
    let end = 4usize
        .checked_add(inputs_bytes)
        .ok_or(Error::TruncatedInput)?;
    // Strict length: same rationale as `parse_vk` / `parse_proof`.
    if bytes.len() != end {
        return Err(Error::TruncatedInput);
    }

    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        out.push(read_fr_at(bytes, 4 + i * FR_SIZE)?);
    }
    Ok(out)
}
