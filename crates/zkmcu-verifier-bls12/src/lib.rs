//! # zkmcu-verifier-bls12 — Groth16 / BLS12-381 on microcontrollers
//!
//! A `no_std` Rust verifier for Groth16 zk-SNARK proofs over the BLS12-381
//! pairing-friendly curve, designed to run on ARM Cortex-M and RISC-V
//! microcontrollers. Wire format is [EIP-2537]: the same bytes that would be
//! verifiable by Ethereum's BLS12-381 precompile. The crypto backend is the
//! zkcrypto [`bls12_381`] crate.
//!
//! Sibling crate to [`zkmcu-verifier`], which does the same job for BN254
//! over the EIP-197 wire format.
//!
//! ## When to use this crate
//!
//! - You need to verify a BLS12-381 Groth16 proof on a device that doesn't
//!   run Linux.
//! - You want the same proof bytes to be verifiable both on-device and by
//!   Ethereum's EIP-2537 precompile.
//! - Your ecosystem is Zcash, Filecoin, or any BLS12-381-based system
//!   (Ethereum sync-committee proofs, IPA-style light clients, etc.).
//!
//! For BN254 stacks (Semaphore, pre-EIP-2537 Ethereum Groth16) use
//! [`zkmcu-verifier`] instead.
//!
//! ## Wire format (EIP-2537)
//!
//! - `Fp`: 64 bytes. 16 leading zero bytes followed by a 48-byte big-endian
//!   integer strictly less than the BLS12-381 base modulus.
//! - `Fr`: 32 bytes big-endian, strictly less than the BLS12-381 scalar
//!   modulus. (Same shape as BN254's `Fr` — the scalar fields are both ~255
//!   bits.)
//! - `G1`: 128 bytes, `x ‖ y`. Point at infinity encodes as all zeros.
//! - `G2`: 256 bytes, `x ‖ y` where each is Fp2 in `(c0 ‖ c1)` order.
//!   **This is the opposite of EIP-197's BN254 convention**, which uses
//!   `(c1 ‖ c0)`. If you are porting code between [`zkmcu-verifier`] and
//!   this crate, check the Fp2 order first.
//! - Verifying key: `alpha(G1) ‖ beta(G2) ‖ gamma(G2) ‖ delta(G2) ‖
//!   num_ic(u32 LE) ‖ ic[num_ic](G1)`.
//! - Proof: `A(G1) ‖ B(G2) ‖ C(G1)`, always [`PROOF_SIZE`] = 512 bytes.
//! - Public inputs: `count(u32 LE) ‖ input[count](Fr)`.
//!
//! ## Typical use
//!
//! ```no_run
//! # fn load(_: &str) -> Vec<u8> { Vec::new() }
//! let ok = zkmcu_verifier_bls12::verify_bytes(
//!     &load("vk.bin"),
//!     &load("proof.bin"),
//!     &load("public.bin"),
//! ).expect("parsed ok");
//! assert!(ok);
//! ```
//!
//! ## Security
//!
//! Same threat model as [`zkmcu-verifier`]: untrusted proof + public-input
//! bytes, trusted VK baked in at firmware-provisioning time. Parsers are
//! DoS-hardened against malicious `num_ic` / `count` fields, and `Fr` /
//! `Fp` canonical-encoding checks are enforced by the underlying
//! [`bls12_381`] types. Not constant-time: `bls12_381`'s pairing varies
//! observably with input. See `SECURITY.md` in the repository root for the
//! full threat model.
//!
//! [EIP-2537]: https://eips.ethereum.org/EIPS/eip-2537
//! [`zkmcu-verifier`]: https://crates.io/crates/zkmcu-verifier

#![no_std]

extern crate alloc;

use alloc::vec::Vec;

use bls12_381::{Bls12, G1Affine, G1Projective, G2Affine, G2Prepared, Scalar};
use group::{Curve, Group};
use pairing::MultiMillerLoop;

pub use bls12_381::{G1Affine as G1, G2Affine as G2, Scalar as Fr};

// ---- Public constants --------------------------------------------------

/// Serialised size of an EIP-2537 `Fp`: 16 zero-padding bytes + 48-byte
/// big-endian integer, strictly less than the BLS12-381 base modulus.
pub const FP_SIZE: usize = 64;
/// Serialised size of an `Fr` scalar. Big-endian; strictly less than the
/// BLS12-381 scalar modulus.
pub const FR_SIZE: usize = 32;
/// Serialised size of an EIP-2537 `G1` point (`x ‖ y`, each 64-byte Fp).
pub const G1_SIZE: usize = FP_SIZE * 2;
/// Serialised size of an EIP-2537 `G2` point (`x ‖ y` where each is Fp2 in
/// `(c0 ‖ c1)` order).
pub const G2_SIZE: usize = FP_SIZE * 4;
/// Serialised size of a Groth16 proof (`A ‖ B ‖ C`). Always 512 bytes.
pub const PROOF_SIZE: usize = G1_SIZE + G2_SIZE + G1_SIZE;

// ---- Error type --------------------------------------------------------

/// Anything the parser or verifier can fail with.
///
/// All variants are recoverable — no panic path ever returns an `Error`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Input buffer shorter than the wire format required, or a claimed
    /// `num_ic` / `count` field pointed past the buffer end.
    TruncatedInput,
    /// An `Fp` encoding had non-zero bytes in the 16-byte leading-zero
    /// padding region, or the 48-byte value was outside the BLS12-381
    /// base modulus.
    InvalidFp,
    /// An `Fr` scalar was ≥ the BLS12-381 scalar modulus (non-canonical
    /// encoding). Rejection is delegated to [`bls12_381::Scalar::from_bytes`],
    /// which enforces strict `< r`.
    InvalidFr,
    /// A `G1` encoding did not decode to a point on the BLS12-381 curve
    /// (or failed subgroup check).
    InvalidG1,
    /// A `G2` encoding did not decode to a point on the BLS12-381 twist
    /// (or failed subgroup check).
    InvalidG2,
    /// `public.len() + 1 != vk.ic.len()` — the number of public inputs
    /// does not match the VK's `gamma_abc_g1` table size.
    PublicInputCount,
}

// ---- Data types --------------------------------------------------------

/// A parsed Groth16 verifying key. Built by [`parse_vk`].
///
/// `ic[0] + Σ public[i] * ic[i+1]` gives the `vk_x` point used by the final
/// pairing check. `ic.len()` is always `public_inputs + 1`.
#[derive(Debug, Clone)]
pub struct VerifyingKey {
    /// `α` in G1.
    pub alpha: G1Affine,
    /// `β` in G2.
    pub beta: G2Affine,
    /// `γ` in G2.
    pub gamma: G2Affine,
    /// `δ` in G2.
    pub delta: G2Affine,
    /// `gamma_abc_g1` — one entry per public input, plus a leading constant term.
    pub ic: Vec<G1Affine>,
}

/// A parsed Groth16 proof. Built by [`parse_proof`].
#[derive(Debug, Clone)]
pub struct Proof {
    /// `A` in G1.
    pub a: G1Affine,
    /// `B` in G2.
    pub b: G2Affine,
    /// `C` in G1.
    pub c: G1Affine,
}

// ---- High-level entry points -------------------------------------------

/// Single-call convenience: parse three byte buffers and run verification.
///
/// Use this when you only verify a given `(vk, proof, public)` triple once.
/// If you verify the same `vk` against many proofs, parse it once with
/// [`parse_vk`] and call [`verify`] repeatedly instead.
///
/// # Errors
///
/// Returns [`Error::TruncatedInput`] if any buffer is shorter than the wire
/// format requires, [`Error::InvalidFp`] / [`Error::InvalidFr`] /
/// [`Error::InvalidG1`] / [`Error::InvalidG2`] for malformed field
/// elements or off-curve points, and [`Error::PublicInputCount`] if the
/// number of public inputs doesn't match the VK's `gamma_abc_g1` table.
pub fn verify_bytes(vk: &[u8], proof: &[u8], public: &[u8]) -> Result<bool, Error> {
    let vk = parse_vk(vk)?;
    let proof = parse_proof(proof)?;
    let public = parse_public(public)?;
    verify(&vk, &proof, &public)
}

/// Verify a Groth16 proof against a verifying key and public inputs.
///
/// Evaluates the pairing product
/// `e(A, B) · e(-α, β) · e(-vk_x, γ) · e(-C, δ)` and checks that it equals
/// the identity in `Gt`, where `vk_x = ic[0] + Σ public[i] · ic[i+1]`.
/// Uses a single [`multi_miller_loop`] call so the four Miller loops share
/// the expensive final exponentiation.
///
/// # Errors
///
/// Returns [`Error::PublicInputCount`] if `public.len() + 1 != vk.ic.len()`.
/// Otherwise infallible — all byte-level validation happens during parsing.
///
/// [`multi_miller_loop`]: pairing::MultiMillerLoop::multi_miller_loop
pub fn verify(vk: &VerifyingKey, proof: &Proof, public: &[Scalar]) -> Result<bool, Error> {
    if public.len() + 1 != vk.ic.len() {
        return Err(Error::PublicInputCount);
    }

    let (ic_head, ic_tail) = vk.ic.split_first().ok_or(Error::PublicInputCount)?;
    let mut acc = G1Projective::from(*ic_head);
    for (scalar, ic_i) in public.iter().zip(ic_tail.iter()) {
        acc += *ic_i * scalar;
    }
    let vk_x = acc.to_affine();

    let neg_alpha = (-G1Projective::from(vk.alpha)).to_affine();
    let neg_vk_x = (-G1Projective::from(vk_x)).to_affine();
    let neg_c = (-G1Projective::from(proof.c)).to_affine();

    let b_prep = G2Prepared::from(proof.b);
    let beta_prep = G2Prepared::from(vk.beta);
    let gamma_prep = G2Prepared::from(vk.gamma);
    let delta_prep = G2Prepared::from(vk.delta);

    let result = Bls12::multi_miller_loop(&[
        (&proof.a, &b_prep),
        (&neg_alpha, &beta_prep),
        (&neg_vk_x, &gamma_prep),
        (&neg_c, &delta_prep),
    ])
    .final_exponentiation();

    Ok(bool::from(result.is_identity()))
}

// ---- Low-level parsing -------------------------------------------------

/// Read an exact-size chunk starting at `offset` with no panic on short input.
fn chunk<const N: usize>(bytes: &[u8], offset: usize) -> Result<&[u8; N], Error> {
    bytes
        .get(offset..)
        .and_then(<[u8]>::first_chunk::<N>)
        .ok_or(Error::TruncatedInput)
}

fn read_u32_le(bytes: &[u8], offset: usize) -> Result<u32, Error> {
    let c = chunk::<4>(bytes, offset)?;
    Ok(u32::from_le_bytes(*c))
}

/// Strip an EIP-2537 `Fp` encoding from 64 bytes to 48 bytes, enforcing
/// that the 16-byte leading-zero padding is all zero. This rejection closes
/// a malleability hole: without the check an attacker could bit-flip the
/// padding bytes and the result would still decode to the same Fp.
fn strip_fp(fp: &[u8; FP_SIZE]) -> Result<[u8; 48], Error> {
    let (pad, value) = fp.split_at(16);
    if pad.iter().any(|&b| b != 0) {
        return Err(Error::InvalidFp);
    }
    let mut out = [0u8; 48];
    let value: &[u8; 48] = value.try_into().expect("FP_SIZE - 16 == 48");
    out.copy_from_slice(value);
    Ok(out)
}

/// Parse a 128-byte `G1` point starting at the given offset. All-zero
/// encodes the point at infinity; any other input is decoded as
/// uncompressed `x ‖ y` and must lie on the curve.
fn read_g1(bytes: &[u8], offset: usize) -> Result<G1Affine, Error> {
    let c: &[u8; G1_SIZE] = chunk::<G1_SIZE>(bytes, offset)?;

    if c.iter().all(|&b| b == 0) {
        return Ok(G1Affine::identity());
    }

    let (x_fp, y_fp) = c.split_at(FP_SIZE);
    let x_fp: &[u8; FP_SIZE] = x_fp.try_into().expect("G1_SIZE == 2 * FP_SIZE");
    let y_fp: &[u8; FP_SIZE] = y_fp.try_into().expect("G1_SIZE == 2 * FP_SIZE");
    let x = strip_fp(x_fp)?;
    let y = strip_fp(y_fp)?;

    // zkcrypto G1 uncompressed = x (48 BE) ‖ y (48 BE), with the top 3 bits
    // of byte 0 being flag bits (compression / infinity / sort). A valid
    // BLS12-381 Fp has all 3 of those bits = 0 because the prime has
    // bit-length 381 < 384, so after strip_fp they're automatically the
    // correct flags for "non-infinity, uncompressed, sort=0".
    let mut zkc = [0u8; 96];
    zkc.get_mut(..48)
        .expect("zkc is 96 bytes")
        .copy_from_slice(&x);
    zkc.get_mut(48..)
        .expect("zkc is 96 bytes")
        .copy_from_slice(&y);
    Option::from(G1Affine::from_uncompressed(&zkc)).ok_or(Error::InvalidG1)
}

/// Parse a 256-byte `G2` point starting at the given offset. All-zero
/// encodes the point at infinity. Non-infinity input is decoded as EIP-2537
/// `(x.c0 ‖ x.c1 ‖ y.c0 ‖ y.c1)`. The decoded bytes are rearranged into
/// zkcrypto's `(x.c1 ‖ x.c0 ‖ y.c1 ‖ y.c0)` order before being handed to
/// [`G2Affine::from_uncompressed`].
fn read_g2(bytes: &[u8], offset: usize) -> Result<G2Affine, Error> {
    let c: &[u8; G2_SIZE] = chunk::<G2_SIZE>(bytes, offset)?;

    if c.iter().all(|&b| b == 0) {
        return Ok(G2Affine::identity());
    }

    let (xc0_fp, rest) = c.split_at(FP_SIZE);
    let (xc1_fp, rest) = rest.split_at(FP_SIZE);
    let (yc0_fp, yc1_fp) = rest.split_at(FP_SIZE);
    let xc0_fp: &[u8; FP_SIZE] = xc0_fp.try_into().expect("G2_SIZE == 4 * FP_SIZE");
    let xc1_fp: &[u8; FP_SIZE] = xc1_fp.try_into().expect("G2_SIZE == 4 * FP_SIZE");
    let yc0_fp: &[u8; FP_SIZE] = yc0_fp.try_into().expect("G2_SIZE == 4 * FP_SIZE");
    let yc1_fp: &[u8; FP_SIZE] = yc1_fp.try_into().expect("G2_SIZE == 4 * FP_SIZE");

    let xc0 = strip_fp(xc0_fp)?;
    let xc1 = strip_fp(xc1_fp)?;
    let yc0 = strip_fp(yc0_fp)?;
    let yc1 = strip_fp(yc1_fp)?;

    // zkcrypto G2 uncompressed expects (c1, c0) Fp2 order.
    let mut zkc = [0u8; 192];
    zkc.get_mut(0..48)
        .expect("zkc is 192 bytes")
        .copy_from_slice(&xc1);
    zkc.get_mut(48..96)
        .expect("zkc is 192 bytes")
        .copy_from_slice(&xc0);
    zkc.get_mut(96..144)
        .expect("zkc is 192 bytes")
        .copy_from_slice(&yc1);
    zkc.get_mut(144..192)
        .expect("zkc is 192 bytes")
        .copy_from_slice(&yc0);
    Option::from(G2Affine::from_uncompressed(&zkc)).ok_or(Error::InvalidG2)
}

/// Parse a 32-byte big-endian scalar. Rejects non-canonical encodings
/// (values ≥ BLS12-381 scalar modulus) because [`Scalar::from_bytes`]
/// enforces strict `< r`.
fn read_fr(bytes: &[u8], offset: usize) -> Result<Scalar, Error> {
    let c: &[u8; FR_SIZE] = chunk::<FR_SIZE>(bytes, offset)?;
    // Wire is big-endian; Scalar::from_bytes expects little-endian.
    let mut le = [0u8; FR_SIZE];
    for (i, &b) in c.iter().enumerate() {
        let idx = FR_SIZE - 1 - i;
        *le.get_mut(idx).expect("FR_SIZE indexing by reverse") = b;
    }
    Option::from(Scalar::from_bytes(&le)).ok_or(Error::InvalidFr)
}

// ---- Container parsing -------------------------------------------------

/// Parse the verifying-key container.
///
/// # Errors
///
/// Returns [`Error::TruncatedInput`] for a short buffer or an adversarial
/// `num_ic` pointing past the buffer end (checked before allocation —
/// prevents a `4 TB`-class `DoS` from a malicious VK). Point / field errors
/// surface as [`Error::InvalidG1`] / [`Error::InvalidG2`] /
/// [`Error::InvalidFp`].
pub fn parse_vk(bytes: &[u8]) -> Result<VerifyingKey, Error> {
    const HEADER: usize = G1_SIZE + 3 * G2_SIZE;

    let alpha = read_g1(bytes, 0)?;
    let beta = read_g2(bytes, G1_SIZE)?;
    let gamma = read_g2(bytes, G1_SIZE + G2_SIZE)?;
    let delta = read_g2(bytes, G1_SIZE + 2 * G2_SIZE)?;

    let num_ic = read_u32_le(bytes, HEADER)? as usize;
    let ic_start = HEADER + 4;

    // Validate num_ic against actual buffer length *before* allocating. On
    // 32-bit targets (MCU) `num_ic * G1_SIZE` could overflow; use checked
    // arithmetic.
    let ic_bytes = num_ic.checked_mul(G1_SIZE).ok_or(Error::TruncatedInput)?;
    let ic_end = ic_start
        .checked_add(ic_bytes)
        .ok_or(Error::TruncatedInput)?;
    if bytes.len() < ic_end {
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

/// Parse an EIP-2537-format proof buffer.
///
/// The buffer must be at least [`PROOF_SIZE`] (512) bytes. Layout:
/// `A(G1) ‖ B(G2) ‖ C(G1)`.
///
/// # Errors
///
/// Same shape as [`parse_vk`]: truncation / malformed field element /
/// off-curve point.
pub fn parse_proof(bytes: &[u8]) -> Result<Proof, Error> {
    let a = read_g1(bytes, 0)?;
    let b = read_g2(bytes, G1_SIZE)?;
    let c = read_g1(bytes, G1_SIZE + G2_SIZE)?;
    Ok(Proof { a, b, c })
}

/// Parse a public-inputs buffer: `count(u32 LE) ‖ input[count](Fr)`.
///
/// DoS-hardened the same way as [`parse_vk`]: the adversary-controlled
/// `count` field is validated against the real buffer length before any
/// allocation.
///
/// # Errors
///
/// [`Error::TruncatedInput`] for short buffers or absurd counts;
/// [`Error::InvalidFr`] for scalars ≥ the scalar modulus.
pub fn parse_public(bytes: &[u8]) -> Result<Vec<Scalar>, Error> {
    let count = read_u32_le(bytes, 0)? as usize;

    let inputs_bytes = count.checked_mul(FR_SIZE).ok_or(Error::TruncatedInput)?;
    let end = 4usize
        .checked_add(inputs_bytes)
        .ok_or(Error::TruncatedInput)?;
    if bytes.len() < end {
        return Err(Error::TruncatedInput);
    }

    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        out.push(read_fr(bytes, 4 + i * FR_SIZE)?);
    }
    Ok(out)
}
