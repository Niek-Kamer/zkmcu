//! PQ-Semaphore custom AIR — Goldilocks × Quadratic alternate config.
//!
//! Sibling to [`crate::pq_semaphore`] (BabyBear × Quartic). Same statement
//! shape (depth-10 Merkle membership + nullifier + scope/signal binding,
//! all hashed with audited Poseidon2-Goldilocks-16); different field
//! and extension. Phase D of the 128-bit security plan exists to compare
//! "BabyBear × Quartic + grinding + d6" (127-bit conjectured, portable)
//! against "Goldilocks × Quadratic + grinding" (127-bit conjectured FRI
//! over a 128-bit native field, no field-side conjecture stacking).
//!
//! ## Why Goldilocks × Quadratic
//!
//! The base-field prime is `2^64 - 2^32 + 1`; `BinomialExtensionField<_, 2>`
//! gives a 128-bit extension. FRI conjectured-soundness from
//! `log_blowup=1 + 64 queries + 16+16 grinding` lands at ~127 bits, same
//! as the BabyBear × Quartic Phase A+B config — but with no
//! conjecture-stack on the field side because the extension already
//! exceeds the 128-bit target natively.
//!
//! Phase 3.3 measured Goldilocks × Quadratic at ~66 % of the
//! BabyBear × Quartic verify cost on M33. Hypothesis under test:
//! 1130 ms M33 (Phase B baseline) → ~600–680 ms.
//!
//! ## Trace layout (16 rows, 13 active)
//!
//! Identical to [`crate::pq_semaphore`]. Row-by-row:
//!
//! ```text
//! row  0       : leaf hash       H(id || 0^12)
//! row  1..=10  : Merkle hops     H(left || right) along the depth-10 path
//! row 11       : Nullifier       H(id || scope)
//! row 12       : Scope binding   H(scope || signal)
//! row 13..=15  : padding         valid zero-input Poseidon2 permutations
//! ```
//!
//! ## Public input layout
//!
//! 4 digest-sized slots × `DIGEST_WIDTH = 4` Goldilocks elements ×
//! 8 bytes/element = 128 bytes:
//!
//! ```text
//! public[ 0.. 4] = merkle_root
//! public[ 4.. 8] = nullifier
//! public[ 8..12] = signal_hash
//! public[12..16] = scope_hash
//! ```
//!
//! `DIGEST_WIDTH = 4` lifts the hash-collision floor to 256 bits of
//! hash space (≈ 128-bit collision under birthday). Smaller digest
//! than the BabyBear-d6 config because each Goldilocks element is 64
//! bits vs BabyBear's 31 bits — 4 × 64 = 256-bit hash space matches
//! BabyBear-d6's 6 × 31 = 186 bits, both above the 128-bit collision
//! security target.
//!
//! ## Lints
//!
//! Mirrors the BabyBear sibling. `clippy::indexing_slicing` is allowed
//! module-wide because every index is into a `[T; N]` of compile-time
//! constant size, bounded by another compile-time constant; the lint
//! can't see through the const-generic shape and the warnings would
//! flood without telling us anything actionable.
//!
//! ## Re-using audited round-evaluation logic
//!
//! Same approach as the BabyBear sibling: Plonky3's `eval_full_round`,
//! `eval_partial_round`, and `eval_sbox` are `pub(crate)` so we copy
//! them verbatim. The audit covers their constraint shape, not their
//! identity as functions; copying preserves audit coverage as long as
//! the bytes match. Each copied helper has a comment citing the
//! upstream path.

#![allow(clippy::indexing_slicing)]
#![allow(clippy::items_after_statements)]
#![allow(clippy::double_must_use)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::doc_lazy_continuation)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::integer_division)]
#![allow(clippy::too_long_first_doc_paragraph)]

use alloc::vec::Vec;
use core::borrow::{Borrow, BorrowMut};
use core::mem::size_of;

use p3_air::{Air, AirBuilder, BaseAir, WindowAccess};
use p3_challenger::DuplexChallenger;
use p3_commit::ExtensionMmcs;
use p3_dft::Radix2DitParallel;
use p3_field::extension::BinomialExtensionField;
use p3_field::{Dup, Field, PrimeCharacteristicRing};
use p3_fri::{FriParameters, TwoAdicFriPcs};
use p3_goldilocks::poseidon1::GOLDILOCKS_S_BOX_DEGREE;
use p3_goldilocks::{
    default_goldilocks_poseidon2_16, GenericPoseidon2LinearLayersGoldilocks, Goldilocks,
    Poseidon2Goldilocks, GOLDILOCKS_POSEIDON2_HALF_FULL_ROUNDS,
    GOLDILOCKS_POSEIDON2_PARTIAL_ROUNDS_16, GOLDILOCKS_POSEIDON2_RC_16_EXTERNAL_FINAL,
    GOLDILOCKS_POSEIDON2_RC_16_EXTERNAL_INITIAL, GOLDILOCKS_POSEIDON2_RC_16_INTERNAL,
};
use p3_merkle_tree::MerkleTreeMmcs;
use p3_poseidon2::GenericPoseidon2LinearLayers;
use p3_poseidon2_air::{FullRound, PartialRound, Poseidon2Cols, RoundConstants, SBox};
use p3_symmetric::{PaddingFreeSponge, Permutation, TruncatedPermutation};
use p3_uni_stark::{verify, StarkConfig};

use crate::Error;

/// State width — width-16 Poseidon2 instance (audit-locked).
pub const WIDTH: usize = 16;
/// Capacity zeros: state slots `[CAPACITY_START..WIDTH]` are always zero
/// on input. Equals `2 * DIGEST_WIDTH` because every active row absorbs
/// at most two digest-sized chunks (e.g. `H(left || right)`); the
/// remaining slots are zero. With `DIGEST_WIDTH=4` and `WIDTH=16` this
/// leaves 8 slots of zero-padding. Not sponge capacity in the
/// absorption sense — every active row is a single fixed-input
/// permutation, not a multi-block sponge — so the audited
/// Poseidon2-Goldilocks-16 round constants remain valid.
pub const CAPACITY_START: usize = 2 * DIGEST_WIDTH;
/// Half external rounds (`R_F / 2 = 4`).
pub const HALF_FULL_ROUNDS: usize = GOLDILOCKS_POSEIDON2_HALF_FULL_ROUNDS;
/// Partial rounds (`R_P = 22`).
pub const PARTIAL_ROUNDS: usize = GOLDILOCKS_POSEIDON2_PARTIAL_ROUNDS_16;
/// S-box degree (`α = 7`).
pub const SBOX_DEGREE: u64 = GOLDILOCKS_S_BOX_DEGREE;
/// One auxiliary register per S-box, matching the canonical Plonky3 example.
pub const SBOX_REGISTERS: usize = 1;

/// Merkle tree depth.
pub const TREE_DEPTH: usize = 10;
/// Total trace rows (next power of two ≥ 13 active rows).
pub const NUM_TRACE_ROWS: usize = 16;
/// Number of public Goldilocks elements (4 digest-sized fields:
/// merkle_root, nullifier, signal_hash, scope_hash).
pub const NUM_PUBLIC_INPUTS: usize = 4 * DIGEST_WIDTH;
/// Public-inputs wire size = `NUM_PUBLIC_INPUTS` × 8 bytes.
pub const PUBLIC_INPUTS_BYTES: usize = NUM_PUBLIC_INPUTS * 8;
/// Digest length (4 Goldilocks elements = 256-bit hash space ≈ 128-bit
/// collision security under birthday — symmetric with the FRI side).
pub const DIGEST_WIDTH: usize = 4;

/// Row index of the leaf hash row.
pub const ROW_LEAF: usize = 0;
/// First Merkle hop row (depth 0 → depth 1).
pub const ROW_MERKLE_FIRST: usize = 1;
/// Last Merkle hop row (root output).
pub const ROW_MERKLE_LAST: usize = 10;
/// Nullifier row.
pub const ROW_NULLIFIER: usize = 11;
/// Scope-binding row.
pub const ROW_SCOPE: usize = 12;

/// Base field type alias. `Val = Goldilocks`. Public because
/// `parse_public_inputs` returns `[Val; NUM_PUBLIC_INPUTS]`, so any
/// caller naming that return type needs the alias.
pub type Val = Goldilocks;
type Challenge = BinomialExtensionField<Val, 2>;
type Perm = Poseidon2Goldilocks<16>;
// Internal trace-MMCS digest is 4 Goldilocks elements = 256 bits, matching
// the AIR-side `DIGEST_WIDTH = 4` and giving 128-bit collision security
// at the FRI Merkle layer. The BabyBear sibling uses 8 elements ×
// 31 bits = 248 bits because BabyBear is too narrow to hit 256 in 4
// elements; with Goldilocks we can. Halving the digest also halves the
// per-query Merkle path bytes — without it, the proof would balloon to
// ~320 KB and exceed `MAX_PROOF_SIZE`.
type FieldHash = PaddingFreeSponge<Perm, 16, 8, 4>;
type Compress = TruncatedPermutation<Perm, 2, 4, 16>;
type ValMmcs =
    MerkleTreeMmcs<<Val as Field>::Packing, <Val as Field>::Packing, FieldHash, Compress, 2, 4>;
type ChallengeMmcs = ExtensionMmcs<Val, Challenge, ValMmcs>;
type Challenger = DuplexChallenger<Val, Perm, 16, 8>;
type Dft = Radix2DitParallel<Val>;
type Pcs = TwoAdicFriPcs<Val, Dft, ValMmcs, ChallengeMmcs>;

/// `StarkConfig` for the Goldilocks-flavoured PQ-Semaphore AIR.
pub type Config = StarkConfig<Pcs, Challenge, Challenger>;
/// Concrete proof type for this AIR.
pub type Proof = p3_uni_stark::Proof<Config>;

// FRI parameters. 64 queries with `log_blowup=1` + 16+16 grinding stack
// to ~127 conjectured bits. Field-side native security 128-bit
// (Goldilocks × Quadratic), so the combined min(127, 128, 128 hash) =
// 127-bit conjectured, with zero conjecture-stack on the field side.
const LOG_BLOWUP: usize = 1;
const NUM_QUERIES: usize = 64;
const COMMIT_POW_BITS: usize = 16;
const QUERY_POW_BITS: usize = 16;
const LOG_FINAL_POLY_LEN: usize = 0;
const MAX_LOG_ARITY: usize = 1;

/// Auxiliary columns laid out after the embedded `Poseidon2Cols`.
///
/// `#[repr(C)]` keeps field offsets stable so `[F]` ↔ `&PqSemaphoreCols<F>`
/// reinterpretation lines up with the column slice the prover/verifier
/// hand to `Air::eval`.
#[repr(C)]
pub struct PqSemaphoreCols<T> {
    /// Embedded Poseidon2 column block — audit-locked layout from
    /// `vendor/Plonky3/poseidon2-air/src/columns.rs`.
    pub poseidon2:
        Poseidon2Cols<T, WIDTH, SBOX_DEGREE, SBOX_REGISTERS, HALF_FULL_ROUNDS, PARTIAL_ROUNDS>,

    /// 1 on Merkle hop rows (rows 1..=10).
    pub is_merkle_hop: T,
    /// 1 on the leaf-hash row (row 0).
    pub is_leaf: T,
    /// 1 on the nullifier row (row 11).
    pub is_nullifier: T,
    /// 1 on the scope-binding row (row 12).
    pub is_scope: T,
    /// 1 on the final Merkle hop (row 10), where output binds to `merkle_root`.
    pub is_root_check: T,
    /// 1 on padding rows (rows 13..=15).
    pub is_padding: T,

    /// Direction bit on Merkle rows (0 = current is left, 1 = current is right).
    pub direction_bit: T,

    /// Continuity column: previous-row digest, copied into the current row's
    /// inputs `[0..DIGEST_WIDTH]` when `direction_bit == 0` else
    /// `[DIGEST_WIDTH..2*DIGEST_WIDTH]`.
    pub prev_digest: [T; DIGEST_WIDTH],
    /// Sibling at this Merkle level. Witness, non-zero on Merkle rows.
    pub sibling: [T; DIGEST_WIDTH],
    /// Identity commitment column. Constant across all rows.
    pub id_col: [T; DIGEST_WIDTH],
    /// Scope column. Constant across all rows.
    pub scope_col: [T; DIGEST_WIDTH],
}

/// Number of auxiliary columns (everything in `PqSemaphoreCols` after
/// the embedded `Poseidon2Cols`).
pub const AUX_COLS: usize = 6 // is_merkle_hop, is_leaf, is_nullifier, is_scope, is_root_check, is_padding
    + 1 // direction_bit
    + DIGEST_WIDTH // prev_digest
    + DIGEST_WIDTH // sibling
    + DIGEST_WIDTH // id_col
    + DIGEST_WIDTH; // scope_col

/// Number of trace columns (computed from `PqSemaphoreCols<u8>` size).
#[must_use]
pub const fn trace_width() -> usize {
    size_of::<PqSemaphoreCols<u8>>()
}

impl<T> Borrow<PqSemaphoreCols<T>> for [T] {
    fn borrow(&self) -> &PqSemaphoreCols<T> {
        debug_assert_eq!(
            core::mem::size_of_val(self),
            size_of::<PqSemaphoreCols<T>>()
        );
        // SAFETY: `PqSemaphoreCols` is `#[repr(C)]`; the embedded
        // `Poseidon2Cols` is also `#[repr(C)]`; the trailing aux fields
        // are all `T` or `[T; N]`. Total size matches `self.len()`
        // entries of `T` — the upstream Plonky3 borrow uses the same
        // alignment trick and the same invariant. Caller guarantees
        // `self.len()` matches `width()`.
        let (prefix, shorts, suffix) = unsafe { self.align_to::<PqSemaphoreCols<T>>() };
        debug_assert!(prefix.is_empty());
        debug_assert!(suffix.is_empty());
        debug_assert_eq!(shorts.len(), 1);
        &shorts[0]
    }
}

impl<T> BorrowMut<PqSemaphoreCols<T>> for [T] {
    fn borrow_mut(&mut self) -> &mut PqSemaphoreCols<T> {
        debug_assert_eq!(
            core::mem::size_of_val(self),
            size_of::<PqSemaphoreCols<T>>()
        );
        // SAFETY: same invariant as the `Borrow` impl above; `align_to_mut`
        // upholds the borrow rules because we hold a `&mut [T]`.
        let (prefix, shorts, suffix) = unsafe { self.align_to_mut::<PqSemaphoreCols<T>>() };
        debug_assert!(prefix.is_empty());
        debug_assert!(suffix.is_empty());
        debug_assert_eq!(shorts.len(), 1);
        &mut shorts[0]
    }
}

/// AIR wrapper around the audited Poseidon2 round constants.
#[derive(Debug)]
pub struct PqSemaphoreAir {
    constants: RoundConstants<Val, WIDTH, HALF_FULL_ROUNDS, PARTIAL_ROUNDS>,
    /// Raw round-constant arrays kept in addition to `RoundConstants`
    /// because the latter has `pub(crate)` fields that we can't read
    /// from outside `p3-poseidon2-air`. Identical bytes — these are
    /// the audited `GOLDILOCKS_POSEIDON2_RC_16_*` constants.
    raw: RawRoundConstants,
}

#[derive(Debug)]
struct RawRoundConstants {
    beginning_full: [[Val; WIDTH]; HALF_FULL_ROUNDS],
    partial: [Val; PARTIAL_ROUNDS],
    ending_full: [[Val; WIDTH]; HALF_FULL_ROUNDS],
}

/// Construct the AIR with audited Poseidon2-Goldilocks round constants.
#[must_use]
pub const fn build_air() -> PqSemaphoreAir {
    let constants = RoundConstants::<Val, WIDTH, HALF_FULL_ROUNDS, PARTIAL_ROUNDS>::new(
        GOLDILOCKS_POSEIDON2_RC_16_EXTERNAL_INITIAL,
        GOLDILOCKS_POSEIDON2_RC_16_INTERNAL,
        GOLDILOCKS_POSEIDON2_RC_16_EXTERNAL_FINAL,
    );
    let raw = RawRoundConstants {
        beginning_full: GOLDILOCKS_POSEIDON2_RC_16_EXTERNAL_INITIAL,
        partial: GOLDILOCKS_POSEIDON2_RC_16_INTERNAL,
        ending_full: GOLDILOCKS_POSEIDON2_RC_16_EXTERNAL_FINAL,
    };
    PqSemaphoreAir { constants, raw }
}

impl PqSemaphoreAir {
    /// Exposed read-only access to the `RoundConstants` for the host
    /// prover's witness-generation routine.
    #[must_use]
    pub const fn constants(&self) -> &RoundConstants<Val, WIDTH, HALF_FULL_ROUNDS, PARTIAL_ROUNDS> {
        &self.constants
    }
}

impl<F: Sync> BaseAir<F> for PqSemaphoreAir {
    fn width(&self) -> usize {
        trace_width()
    }

    fn num_public_values(&self) -> usize {
        NUM_PUBLIC_INPUTS
    }

    fn main_next_row_columns(&self) -> Vec<usize> {
        (0..trace_width()).collect()
    }

    fn max_constraint_degree(&self) -> Option<usize> {
        None
    }
}

impl<AB: AirBuilder<F = Val>> Air<AB> for PqSemaphoreAir {
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();
        let local_slice = main.current_slice();
        let next_slice = main.next_slice();
        let local: &PqSemaphoreCols<AB::Var> = local_slice.borrow();
        let next: &PqSemaphoreCols<AB::Var> = next_slice.borrow();

        // 1. Audited Poseidon2 round constraints.
        eval_poseidon2::<AB>(&self.raw, builder, &local.poseidon2);

        let public: [AB::PublicVar; NUM_PUBLIC_INPUTS] = {
            let pis = builder.public_values();
            debug_assert!(pis.len() == NUM_PUBLIC_INPUTS);
            core::array::from_fn(|i| pis[i])
        };

        // 2. Selector booleanity.
        builder.assert_bool(local.is_merkle_hop);
        builder.assert_bool(local.is_leaf);
        builder.assert_bool(local.is_nullifier);
        builder.assert_bool(local.is_scope);
        builder.assert_bool(local.is_root_check);
        builder.assert_bool(local.is_padding);
        builder.assert_bool(local.direction_bit);

        // 3. Selectors mutually exclusive.
        let one_active = local.is_leaf.into()
            + local.is_merkle_hop.into()
            + local.is_nullifier.into()
            + local.is_scope.into()
            + local.is_padding.into();
        builder.assert_eq(one_active, AB::Expr::ONE);

        // is_root_check ⇒ is_merkle_hop
        builder
            .assert_zero(local.is_root_check.into() * (AB::Expr::ONE - local.is_merkle_hop.into()));

        // 4. Capacity zeros on active rows.
        let active = AB::Expr::ONE - local.is_padding.into();
        for j in CAPACITY_START..WIDTH {
            builder.assert_zero(active.dup() * local.poseidon2.inputs[j].into());
        }

        // 5. Leaf row: inputs[0..DIGEST_WIDTH] == id_col, inputs[DIGEST_WIDTH..2*DIGEST_WIDTH] == 0.
        for j in 0..DIGEST_WIDTH {
            builder.assert_zero(
                local.is_leaf.into() * (local.poseidon2.inputs[j].into() - local.id_col[j].into()),
            );
            builder.assert_zero(
                local.is_leaf.into() * local.poseidon2.inputs[j + DIGEST_WIDTH].into(),
            );
        }

        // 6. Merkle hop conditional swap.
        let dir = local.direction_bit.into();
        let one_minus_dir = AB::Expr::ONE - dir.dup();
        for j in 0..DIGEST_WIDTH {
            let lhs0 = local.prev_digest[j].into() * one_minus_dir.dup()
                + local.sibling[j].into() * dir.dup();
            let lhs1 = local.prev_digest[j].into() * dir.dup()
                + local.sibling[j].into() * one_minus_dir.dup();
            builder.assert_zero(
                local.is_merkle_hop.into() * (local.poseidon2.inputs[j].into() - lhs0),
            );
            builder.assert_zero(
                local.is_merkle_hop.into()
                    * (local.poseidon2.inputs[j + DIGEST_WIDTH].into() - lhs1),
            );
        }

        // 7. Nullifier row: inputs[0..DIGEST_WIDTH] = id, inputs[DIGEST_WIDTH..2*DIGEST_WIDTH] = scope.
        for j in 0..DIGEST_WIDTH {
            builder.assert_zero(
                local.is_nullifier.into()
                    * (local.poseidon2.inputs[j].into() - local.id_col[j].into()),
            );
            builder.assert_zero(
                local.is_nullifier.into()
                    * (local.poseidon2.inputs[j + DIGEST_WIDTH].into() - local.scope_col[j].into()),
            );
        }

        // 8. Scope-binding row.
        for j in 0..DIGEST_WIDTH {
            builder.assert_zero(
                local.is_scope.into()
                    * (local.poseidon2.inputs[j].into() - local.scope_col[j].into()),
            );
            let signal = public[2 * DIGEST_WIDTH + j].into();
            builder.assert_zero(
                local.is_scope.into() * (local.poseidon2.inputs[j + DIGEST_WIDTH].into() - signal),
            );
        }

        // 9. Public-input bindings on output.
        let final_full = HALF_FULL_ROUNDS - 1;
        for j in 0..DIGEST_WIDTH {
            let post = local.poseidon2.ending_full_rounds[final_full].post[j].into();

            let root = public[j].into();
            builder.assert_zero(local.is_root_check.into() * (post.dup() - root));

            let nullifier = public[DIGEST_WIDTH + j].into();
            builder.assert_zero(local.is_nullifier.into() * (post.dup() - nullifier));

            let scope_hash = public[3 * DIGEST_WIDTH + j].into();
            builder.assert_zero(local.is_scope.into() * (post - scope_hash));
        }

        // 10. Inter-row continuity.
        let final_full = HALF_FULL_ROUNDS - 1;
        let mut t = builder.when_transition();
        for j in 0..DIGEST_WIDTH {
            t.assert_eq(local.id_col[j], next.id_col[j]);
            t.assert_eq(local.scope_col[j], next.scope_col[j]);

            let cur_post = local.poseidon2.ending_full_rounds[final_full].post[j].into();
            t.assert_zero(next.is_merkle_hop.into() * (next.prev_digest[j].into() - cur_post));
        }
    }
}

// ============================================================================
// Build a `Config` (audited Poseidon2-Goldilocks perm; 64 queries; 127 conj).
// ============================================================================

/// Build a fresh `Config` from the audited Poseidon2-Goldilocks permutation.
#[must_use]
pub fn make_config() -> Config {
    let perm = default_goldilocks_poseidon2_16();
    let hash = FieldHash::new(perm.clone());
    let compress = Compress::new(perm.clone());
    let val_mmcs = ValMmcs::new(hash, compress, 0);
    let challenge_mmcs = ChallengeMmcs::new(val_mmcs.clone());
    let dft = Dft::default();
    let fri_params = FriParameters {
        log_blowup: LOG_BLOWUP,
        log_final_poly_len: LOG_FINAL_POLY_LEN,
        max_log_arity: MAX_LOG_ARITY,
        num_queries: NUM_QUERIES,
        commit_proof_of_work_bits: COMMIT_POW_BITS,
        query_proof_of_work_bits: QUERY_POW_BITS,
        mmcs: challenge_mmcs,
    };
    let pcs = Pcs::new(dft, val_mmcs, fri_params);
    let challenger = Challenger::new(perm);
    Config::new(pcs, challenger)
}

// ============================================================================
// Wire format: parse / encode proof + public inputs.
// ============================================================================

/// Deserialise a postcard-encoded proof.
///
/// # Errors
///
/// Returns [`Error::ProofDeserialization`] on length-cap violation,
/// postcard decode failure, or trailing bytes.
pub fn parse_proof(bytes: &[u8]) -> Result<Proof, Error> {
    if bytes.len() > crate::MAX_PROOF_SIZE {
        return Err(Error::ProofDeserialization);
    }
    let (proof, rest) =
        postcard::take_from_bytes::<Proof>(bytes).map_err(|_| Error::ProofDeserialization)?;
    if !rest.is_empty() {
        return Err(Error::ProofDeserialization);
    }
    Ok(proof)
}

/// Encode a proof to bytes via postcard.
///
/// # Errors
///
/// Returns [`Error::ProofDeserialization`] if postcard rejects the encode
/// (in practice only on allocator OOM).
pub fn encode_proof(proof: &Proof) -> Result<Vec<u8>, Error> {
    postcard::to_allocvec(proof).map_err(|_| Error::ProofDeserialization)
}

/// Parse the 128-byte public-inputs blob into 16 canonical Goldilocks
/// elements (little-endian, 8 bytes each).
///
/// # Errors
///
/// Returns [`Error::PublicDeserialization`] if the byte length is wrong
/// or any chunk decodes to a non-canonical value (`>= p`).
pub fn parse_public_inputs(bytes: &[u8]) -> Result<[Val; NUM_PUBLIC_INPUTS], Error> {
    if bytes.len() != PUBLIC_INPUTS_BYTES {
        return Err(Error::PublicDeserialization);
    }
    let mut out = [Val::ZERO; NUM_PUBLIC_INPUTS];
    // Goldilocks prime: 2^64 - 2^32 + 1 = 0xFFFF_FFFF_0000_0001.
    const GOLDILOCKS_PRIME: u64 = 0xFFFF_FFFF_0000_0001;
    for (i, chunk) in bytes.chunks_exact(8).enumerate() {
        let arr: [u8; 8] = chunk.try_into().map_err(|_| Error::PublicDeserialization)?;
        let v = u64::from_le_bytes(arr);
        if v >= GOLDILOCKS_PRIME {
            return Err(Error::PublicDeserialization);
        }
        if let Some(slot) = out.get_mut(i) {
            *slot = Val::new(v);
        } else {
            return Err(Error::PublicDeserialization);
        }
    }
    Ok(out)
}

// ============================================================================
// Verify entry points.
// ============================================================================

/// Verify with externally-built `Config` + `Air` + parsed public inputs.
///
/// # Errors
///
/// Returns [`Error::VerificationFailed`] if Plonky3 rejects the proof.
pub fn verify_with_config(
    proof: &Proof,
    public: &[Val; NUM_PUBLIC_INPUTS],
    config: &Config,
    air: &PqSemaphoreAir,
) -> Result<(), Error> {
    verify(config, air, proof, &public[..]).map_err(|_| Error::VerificationFailed)
}

/// Verify a proof from raw bytes + public-inputs bytes, building config / air
/// internally.
///
/// # Errors
///
/// Propagates [`Error::ProofDeserialization`], [`Error::PublicDeserialization`],
/// or [`Error::VerificationFailed`].
pub fn parse_and_verify(proof_bytes: &[u8], public_bytes: &[u8]) -> Result<(), Error> {
    let proof = parse_proof(proof_bytes)?;
    let public = parse_public_inputs(public_bytes)?;
    let config = make_config();
    let air = build_air();
    verify_with_config(&proof, &public, &config, &air)
}

/// Run the audited Poseidon2-Goldilocks permutation and return the full
/// 16-element output state.
pub fn poseidon2_permute(state: [Val; WIDTH]) -> [Val; WIDTH] {
    let perm = default_goldilocks_poseidon2_16();
    let mut out = state;
    perm.permute_mut(&mut out);
    out
}

// ============================================================================
// Witness / trace construction helpers.
// ============================================================================

/// Witness for one PQ-Semaphore proof.
#[derive(Clone, Debug)]
pub struct PqSemaphoreWitness {
    /// Identity commitment.
    pub id: [Val; DIGEST_WIDTH],
    /// Application scope.
    pub scope: [Val; DIGEST_WIDTH],
    /// Pre-hashed message.
    pub signal_hash: [Val; DIGEST_WIDTH],
    /// 10 sibling values along the Merkle path.
    pub siblings: [[Val; DIGEST_WIDTH]; TREE_DEPTH],
    /// 10 direction bits along the Merkle path.
    pub direction_bits: [bool; TREE_DEPTH],
    /// Computed leaf = H(id || 0^(WIDTH - DIGEST_WIDTH)).
    pub leaf: [Val; DIGEST_WIDTH],
    /// Computed Merkle root.
    pub merkle_root: [Val; DIGEST_WIDTH],
    /// Computed nullifier = H(id || scope).
    pub nullifier: [Val; DIGEST_WIDTH],
    /// Computed scope_hash = H(scope || message).
    pub scope_hash: [Val; DIGEST_WIDTH],
    /// Per-Merkle-level intermediate digests (after each hop).
    pub path_digests: [[Val; DIGEST_WIDTH]; TREE_DEPTH],
}

/// Hash `(input || 0^(WIDTH - DIGEST_WIDTH))` for a Merkle leaf.
#[must_use]
pub fn hash_leaf(input: [Val; DIGEST_WIDTH]) -> [Val; DIGEST_WIDTH] {
    let mut state = [Val::ZERO; WIDTH];
    state[..DIGEST_WIDTH].copy_from_slice(&input);
    let out = poseidon2_permute(state);
    let mut digest = [Val::ZERO; DIGEST_WIDTH];
    digest.copy_from_slice(&out[..DIGEST_WIDTH]);
    digest
}

/// Hash `(left || right)` with capacity zero.
#[must_use]
pub fn hash_pair(left: [Val; DIGEST_WIDTH], right: [Val; DIGEST_WIDTH]) -> [Val; DIGEST_WIDTH] {
    let mut state = [Val::ZERO; WIDTH];
    state[..DIGEST_WIDTH].copy_from_slice(&left);
    state[DIGEST_WIDTH..2 * DIGEST_WIDTH].copy_from_slice(&right);
    let out = poseidon2_permute(state);
    let mut digest = [Val::ZERO; DIGEST_WIDTH];
    digest.copy_from_slice(&out[..DIGEST_WIDTH]);
    digest
}

/// Build a deterministic witness for the canonical "leaf 0 of a depth-10
/// tree" scenario.
#[must_use]
pub fn build_witness(
    id: [Val; DIGEST_WIDTH],
    scope: [Val; DIGEST_WIDTH],
    signal_hash: [Val; DIGEST_WIDTH],
) -> PqSemaphoreWitness {
    let leaf = hash_leaf(id);

    let total_leaves: usize = 1 << TREE_DEPTH;
    let mut nodes: Vec<[Val; DIGEST_WIDTH]> = (0..total_leaves)
        .map(|i| {
            if i == 0 {
                leaf
            } else {
                let mut v = [Val::ZERO; DIGEST_WIDTH];
                #[allow(clippy::cast_possible_truncation)]
                let i_u64 = i as u64;
                v[0] = Val::new(i_u64);
                hash_leaf(v)
            }
        })
        .collect();

    let mut siblings = [[Val::ZERO; DIGEST_WIDTH]; TREE_DEPTH];
    let mut path_digests = [[Val::ZERO; DIGEST_WIDTH]; TREE_DEPTH];
    let mut current = leaf;
    let direction_bits = [false; TREE_DEPTH];
    for level in 0..TREE_DEPTH {
        let sibling = nodes[1];
        siblings[level] = sibling;
        let merged = hash_pair(current, sibling);
        path_digests[level] = merged;
        current = merged;

        let next_len = nodes.len() / 2;
        let mut next = Vec::with_capacity(next_len);
        for i in 0..next_len {
            next.push(hash_pair(nodes[2 * i], nodes[2 * i + 1]));
        }
        nodes = next;
    }

    let merkle_root = current;
    let nullifier = hash_pair(id, scope);
    let scope_hash = hash_pair(scope, signal_hash);

    PqSemaphoreWitness {
        id,
        scope,
        signal_hash,
        siblings,
        direction_bits,
        leaf,
        merkle_root,
        nullifier,
        scope_hash,
        path_digests,
    }
}

/// Pack a witness into the `NUM_PUBLIC_INPUTS`-element public-input array.
#[must_use]
pub fn pack_public_inputs(witness: &PqSemaphoreWitness) -> [Val; NUM_PUBLIC_INPUTS] {
    let mut out = [Val::ZERO; NUM_PUBLIC_INPUTS];
    let d = DIGEST_WIDTH;
    out[0..d].copy_from_slice(&witness.merkle_root);
    out[d..2 * d].copy_from_slice(&witness.nullifier);
    out[2 * d..3 * d].copy_from_slice(&witness.signal_hash);
    out[3 * d..4 * d].copy_from_slice(&witness.scope_hash);
    out
}

/// Encode a packed public-inputs array into the 128-byte little-endian wire
/// format consumed by [`parse_public_inputs`].
#[must_use]
pub fn encode_public_inputs(public: &[Val; NUM_PUBLIC_INPUTS]) -> Vec<u8> {
    use p3_field::PrimeField64;
    let mut out = Vec::with_capacity(PUBLIC_INPUTS_BYTES);
    for v in public {
        out.extend_from_slice(&v.as_canonical_u64().to_le_bytes());
    }
    out
}

/// Build the trace matrix for a given witness.
#[must_use]
pub fn build_trace_values(witness: &PqSemaphoreWitness) -> Vec<Val> {
    let width = trace_width();
    let total = NUM_TRACE_ROWS * width;
    let mut data = alloc::vec![Val::ZERO; total];

    let air = build_air();
    let _constants = air.constants();

    for row_idx in 0..NUM_TRACE_ROWS {
        let row_slice = &mut data[row_idx * width..(row_idx + 1) * width];
        let cols: &mut PqSemaphoreCols<Val> = row_slice.borrow_mut();

        cols.id_col = witness.id;
        cols.scope_col = witness.scope;

        let (state, kind) = match row_idx {
            0 /* ROW_LEAF */ => {
                let mut s = [Val::ZERO; WIDTH];
                s[..DIGEST_WIDTH].copy_from_slice(&witness.id);
                (s, RowKind::Leaf)
            }
            r if (ROW_MERKLE_FIRST..=ROW_MERKLE_LAST).contains(&r) => {
                let level = r - ROW_MERKLE_FIRST;
                let prev = if level == 0 {
                    witness.leaf
                } else {
                    witness.path_digests[level - 1]
                };
                let sibling = witness.siblings[level];
                let dir = witness.direction_bits[level];
                let (left, right) = if dir { (sibling, prev) } else { (prev, sibling) };
                let mut s = [Val::ZERO; WIDTH];
                s[..DIGEST_WIDTH].copy_from_slice(&left);
                s[DIGEST_WIDTH..2 * DIGEST_WIDTH].copy_from_slice(&right);
                cols.prev_digest = prev;
                cols.sibling = sibling;
                cols.direction_bit = if dir { Val::ONE } else { Val::ZERO };
                let kind = if r == ROW_MERKLE_LAST {
                    RowKind::MerkleRoot
                } else {
                    RowKind::Merkle
                };
                (s, kind)
            }
            r if r == ROW_NULLIFIER => {
                let mut s = [Val::ZERO; WIDTH];
                s[..DIGEST_WIDTH].copy_from_slice(&witness.id);
                s[DIGEST_WIDTH..2 * DIGEST_WIDTH].copy_from_slice(&witness.scope);
                (s, RowKind::Nullifier)
            }
            r if r == ROW_SCOPE => {
                let mut s = [Val::ZERO; WIDTH];
                s[..DIGEST_WIDTH].copy_from_slice(&witness.scope);
                s[DIGEST_WIDTH..2 * DIGEST_WIDTH].copy_from_slice(&witness.signal_hash);
                (s, RowKind::Scope)
            }
            _ => ([Val::ZERO; WIDTH], RowKind::Padding),
        };

        cols.is_leaf = Val::ZERO;
        cols.is_merkle_hop = Val::ZERO;
        cols.is_nullifier = Val::ZERO;
        cols.is_scope = Val::ZERO;
        cols.is_root_check = Val::ZERO;
        cols.is_padding = Val::ZERO;
        match kind {
            RowKind::Leaf => cols.is_leaf = Val::ONE,
            RowKind::Merkle => cols.is_merkle_hop = Val::ONE,
            RowKind::MerkleRoot => {
                cols.is_merkle_hop = Val::ONE;
                cols.is_root_check = Val::ONE;
            }
            RowKind::Nullifier => cols.is_nullifier = Val::ONE,
            RowKind::Scope => cols.is_scope = Val::ONE,
            RowKind::Padding => cols.is_padding = Val::ONE,
        }

        fill_poseidon2_row(&mut cols.poseidon2, state);
    }

    data
}

#[derive(Clone, Copy)]
enum RowKind {
    Leaf,
    Merkle,
    MerkleRoot,
    Nullifier,
    Scope,
    Padding,
}

/// Run one full Poseidon2 permutation and write the audited round-by-round
/// columns into `dst`.
fn fill_poseidon2_row(
    dst: &mut Poseidon2Cols<
        Val,
        WIDTH,
        SBOX_DEGREE,
        SBOX_REGISTERS,
        HALF_FULL_ROUNDS,
        PARTIAL_ROUNDS,
    >,
    mut state: [Val; WIDTH],
) {
    type LL = GenericPoseidon2LinearLayersGoldilocks;

    dst.inputs = state;

    LL::external_linear_layer(&mut state);

    for round_idx in 0..HALF_FULL_ROUNDS {
        let rc = &GOLDILOCKS_POSEIDON2_RC_16_EXTERNAL_INITIAL[round_idx];
        let full_round = &mut dst.beginning_full_rounds[round_idx];
        for (i, (s, r)) in state.iter_mut().zip(rc.iter()).enumerate() {
            *s += *r;
            let x = *s;
            let x3 = x.cube();
            full_round.sbox[i].0[0] = x3;
            *s = x3 * x3 * x;
        }
        LL::external_linear_layer(&mut state);
        full_round.post = state;
    }

    for round_idx in 0..PARTIAL_ROUNDS {
        let rc = GOLDILOCKS_POSEIDON2_RC_16_INTERNAL[round_idx];
        let pr = &mut dst.partial_rounds[round_idx];
        state[0] += rc;
        let x = state[0];
        let x3 = x.cube();
        pr.sbox.0[0] = x3;
        state[0] = x3 * x3 * x;
        pr.post_sbox = state[0];
        LL::internal_linear_layer(&mut state);
    }

    for round_idx in 0..HALF_FULL_ROUNDS {
        let rc = &GOLDILOCKS_POSEIDON2_RC_16_EXTERNAL_FINAL[round_idx];
        let full_round = &mut dst.ending_full_rounds[round_idx];
        for (i, (s, r)) in state.iter_mut().zip(rc.iter()).enumerate() {
            *s += *r;
            let x = *s;
            let x3 = x.cube();
            full_round.sbox[i].0[0] = x3;
            *s = x3 * x3 * x;
        }
        LL::external_linear_layer(&mut state);
        full_round.post = state;
    }
}

// ============================================================================
// Audited Poseidon2 round-evaluation logic.
//
// Copied verbatim from `vendor/Plonky3/poseidon2-air/src/air.rs:144-323`,
// retyped for `Val = Goldilocks` and the Goldilocks linear-layer impl.
// ============================================================================

fn eval_poseidon2<AB>(
    raw: &RawRoundConstants,
    builder: &mut AB,
    local: &Poseidon2Cols<
        AB::Var,
        WIDTH,
        SBOX_DEGREE,
        SBOX_REGISTERS,
        HALF_FULL_ROUNDS,
        PARTIAL_ROUNDS,
    >,
) where
    AB: AirBuilder<F = Val>,
{
    let mut state: [_; WIDTH] = local.inputs.map(Into::into);

    GenericPoseidon2LinearLayersGoldilocks::external_linear_layer(&mut state);

    for round in 0..HALF_FULL_ROUNDS {
        eval_full_round::<AB>(
            &mut state,
            &local.beginning_full_rounds[round],
            &raw.beginning_full[round],
            builder,
        );
    }

    for round in 0..PARTIAL_ROUNDS {
        eval_partial_round::<AB>(
            &mut state,
            &local.partial_rounds[round],
            &raw.partial[round],
            builder,
        );
    }

    for round in 0..HALF_FULL_ROUNDS {
        eval_full_round::<AB>(
            &mut state,
            &local.ending_full_rounds[round],
            &raw.ending_full[round],
            builder,
        );
    }
}

#[inline]
fn eval_full_round<AB>(
    state: &mut [AB::Expr; WIDTH],
    full_round: &FullRound<AB::Var, WIDTH, SBOX_DEGREE, SBOX_REGISTERS>,
    round_constants: &[AB::F; WIDTH],
    builder: &mut AB,
) where
    AB: AirBuilder<F = Val>,
{
    for (i, (s, r)) in state.iter_mut().zip(round_constants.iter()).enumerate() {
        *s += r.dup();
        eval_sbox(&full_round.sbox[i], s, builder);
    }
    GenericPoseidon2LinearLayersGoldilocks::external_linear_layer(state);
    for (state_i, post_i) in state.iter_mut().zip(full_round.post) {
        builder.assert_eq(state_i.clone(), post_i);
        *state_i = post_i.into();
    }
}

#[inline]
fn eval_partial_round<AB>(
    state: &mut [AB::Expr; WIDTH],
    partial_round: &PartialRound<AB::Var, WIDTH, SBOX_DEGREE, SBOX_REGISTERS>,
    round_constant: &AB::F,
    builder: &mut AB,
) where
    AB: AirBuilder<F = Val>,
{
    state[0] += round_constant.dup();
    eval_sbox(&partial_round.sbox, &mut state[0], builder);

    builder.assert_eq(state[0].dup(), partial_round.post_sbox);
    state[0] = partial_round.post_sbox.into();

    GenericPoseidon2LinearLayersGoldilocks::internal_linear_layer(state);
}

#[inline]
fn eval_sbox<AB>(
    sbox: &SBox<AB::Var, SBOX_DEGREE, SBOX_REGISTERS>,
    x: &mut AB::Expr,
    builder: &mut AB,
) where
    AB: AirBuilder,
{
    let committed_x3 = sbox.0[0].into();
    builder.assert_eq(committed_x3.dup(), x.cube());
    *x = committed_x3.square() * x.dup();
}
