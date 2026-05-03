//! PQ-Semaphore custom AIR — depth-10 Merkle membership + nullifier +
//! scope/signal binding, all hashed with audited Poseidon2-`BabyBear`.
//!
//! ## Design (Option A, embedded `Poseidon2Cols`)
//!
//! See `research/notebook/2026-04-29-pq-semaphore-air-design.md` for the
//! full design rationale. Short version: every trace row is one
//! Poseidon2 permutation. The audited Plonky3 [`Poseidon2Cols`] layout
//! is embedded inside our [`PqSemaphoreCols`] struct so the audited
//! constraint surface is unchanged. We add a handful of auxiliary
//! columns + cross-row equality checks for Merkle wiring, nullifier,
//! scope binding, and identity / scope continuity.
//!
//! ## Trace layout (16 rows total, 13 active)
//!
//! ```text
//! row  0 : leaf hash       H(id || 0^10)             — leaf = post[0..6]
//! row  1 : Merkle hop  0   H(left || right)          — depth-0 -> depth-1
//! row  2 : Merkle hop  1   ...
//! ...
//! row 10 : Merkle hop  9   final hop, post[0..6] == merkle_root
//! row 11 : Nullifier       H(id || scope)            — post[0..6] == nullifier
//! row 12 : Scope binding   H(scope || message)       — post[0..6] == scope_hash
//! row 13-15 : padding (valid zero-input Poseidon2 permutations)
//! ```
//!
//! ## Public input layout (`24` `BabyBear` elements, 96 bytes on disk)
//!
//! ```text
//! public[ 0.. 6] = merkle_root[0..6]
//! public[ 6..12] = nullifier[0..6]
//! public[12..18] = signal_hash[0..6]
//! public[18..24] = scope_hash[0..6]
//! ```
//!
//! ## Design adjustments during implementation
//!
//! The original design doc (committed as `91afa38`) underspecified
//! several things. They are documented here, in the AIR module
//! comments, AND amended into the design doc under
//! `## Design adjustments during implementation`.
//!
//! 1. **A 13th active row is needed for the leaf hash.** The doc had
//!    rows 0..10 for Merkle hops + row 10 nullifier + row 11 scope
//!    binding. But the Merkle path's leaf is `H(id)`, which itself
//!    requires a Poseidon2 row. We added row 0 = leaf hash; the Merkle
//!    hops shift to rows 1..=10; nullifier moves to row 11; scope
//!    binding moves to row 12; padding rows 13..=15.
//!
//! 2. **Cross-row equality (id, scope) needs witness columns.**
//!    The nullifier (row 11) and scope binding (row 12) need to share
//!    the same `scope` value, and the leaf hash (row 0) and the
//!    nullifier (row 11) need to share the same `id`. `uni-stark`
//!    only supports adjacent-row transition constraints, so we
//!    introduce per-row witness columns `id_col[0..4]` and
//!    `scope_col[0..4]` that hold those values on every row, with
//!    transition constraints enforcing they're constant across the
//!    whole trace. The active rows then bind their input slots to
//!    those witness columns.
//!
//! 3. **Sibling and prev_digest are witness columns.** The Merkle
//!    hop row's `prev_digest` (continuity from the previous hop) and
//!    `sibling[0..4]` are added as witness columns. On row 0
//!    (leaf hash) the prev_digest column is unconstrained.
//!
//! 4. **Padding rows are valid zero-input Poseidon2.** Selectors
//!    are all zero on padding rows; conditional-swap, continuity,
//!    and binding constraints are all gated by the corresponding
//!    selectors so padding rows pass vacuously while still satisfying
//!    the audited Poseidon2 constraint surface.
//!
//! 5. **Final column count: ~332 columns** =
//!    298 (Poseidon2) + 7 (selectors / direction bit) + 6 (prev_digest)
//!    + 6 (sibling) + 6 (id_col) + 6 (scope_col). The `width()` impl
//!    returns `size_of::<PqSemaphoreCols<u8>>()` so the column count is
//!    single-source-of-truth and rescales automatically with
//!    [`DIGEST_WIDTH`].
//!
//! ## Lints
//!
//! `clippy::indexing_slicing` is allowed module-wide: every index in this
//! file is into a `[T; N]` where `N` is a compile-time constant and the
//! index is bounded by another compile-time constant (the round counters,
//! the digest width, the public-input layout). The lint can't see through
//! the const-generic shape of `Poseidon2Cols`, so the warnings would
//! flood without telling us anything actionable. The Plonky3 vendored
//! copy of the same logic doesn't trip clippy because Plonky3 silences
//! these via its workspace-level lint config; we silence module-locally
//! to scope the allow tightly.
//!
//! ## Re-using audited round-evaluation logic
//!
//! Plonky3's `eval_full_round`, `eval_partial_round`, and `eval_sbox`
//! are `pub(crate)` so they can't be called from outside the
//! `p3_poseidon2_air` crate. We copy them verbatim (the audit covers
//! their constraint shape, not their identity as functions; copying
//! them preserves audit coverage as long as the bytes match). Each
//! copied helper has a comment citing the upstream path.

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
use p3_baby_bear::{
    default_babybear_poseidon2_16, BabyBear, GenericPoseidon2LinearLayersBabyBear,
    Poseidon2BabyBear, BABYBEAR_POSEIDON2_HALF_FULL_ROUNDS, BABYBEAR_POSEIDON2_PARTIAL_ROUNDS_16,
    BABYBEAR_POSEIDON2_RC_16_EXTERNAL_FINAL, BABYBEAR_POSEIDON2_RC_16_EXTERNAL_INITIAL,
    BABYBEAR_POSEIDON2_RC_16_INTERNAL, BABYBEAR_S_BOX_DEGREE,
};
use p3_challenger::DuplexChallenger;
use p3_commit::ExtensionMmcs;
use p3_dft::Radix2DitParallel;
use p3_field::extension::BinomialExtensionField;
use p3_field::{Dup, Field, PrimeCharacteristicRing};
use p3_fri::{FriParameters, TwoAdicFriPcs};
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
/// remaining slots are zero. With digest=6 and width=16 this leaves
/// 4 slots of zero-padding. This is *not* sponge capacity in the
/// absorption sense — every active row is a single fixed-input
/// permutation, not a multi-block sponge — so the audited
/// Poseidon2-BabyBear-16 round constants remain valid.
pub const CAPACITY_START: usize = 2 * DIGEST_WIDTH;
/// Half external rounds (`R_F / 2 = 4`).
pub const HALF_FULL_ROUNDS: usize = BABYBEAR_POSEIDON2_HALF_FULL_ROUNDS;
/// Partial rounds (`R_P = 13`).
pub const PARTIAL_ROUNDS: usize = BABYBEAR_POSEIDON2_PARTIAL_ROUNDS_16;
/// S-box degree (`α = 7`).
pub const SBOX_DEGREE: u64 = BABYBEAR_S_BOX_DEGREE;
/// One auxiliary register per S-box, matching the canonical Plonky3 example.
pub const SBOX_REGISTERS: usize = 1;

/// Merkle tree depth.
pub const TREE_DEPTH: usize = 10;
/// Total trace rows (next power of two ≥ 13 active rows).
pub const NUM_TRACE_ROWS: usize = 16;
/// Number of public BabyBear elements (4 digest-sized fields:
/// merkle_root, nullifier, signal_hash, scope_hash).
pub const NUM_PUBLIC_INPUTS: usize = 4 * DIGEST_WIDTH;
/// Public-inputs wire size = `NUM_PUBLIC_INPUTS` × 4 bytes.
pub const PUBLIC_INPUTS_BYTES: usize = NUM_PUBLIC_INPUTS * 4;
/// Digest length (6 `BabyBear` elements = 186-bit digest space, ~93-bit
/// generic-collision resistance — removes the hash-collision floor as the
/// soundness bottleneck for the 128-bit-class target).
pub const DIGEST_WIDTH: usize = 6;

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

/// Base field type alias. `Val = BabyBear`. Public because
/// `parse_public_inputs` returns `[Val; NUM_PUBLIC_INPUTS]`, so any
/// caller naming that return type needs the alias.
pub type Val = BabyBear;
type Challenge = BinomialExtensionField<Val, 4>;
type Perm = Poseidon2BabyBear<16>;
type FieldHash = PaddingFreeSponge<Perm, 16, 8, 8>;
type Compress = TruncatedPermutation<Perm, 2, 8, 16>;
type ValMmcs =
    MerkleTreeMmcs<<Val as Field>::Packing, <Val as Field>::Packing, FieldHash, Compress, 2, 8>;
type ChallengeMmcs = ExtensionMmcs<Val, Challenge, ValMmcs>;
type Challenger = DuplexChallenger<Val, Perm, 16, 8>;
type Dft = Radix2DitParallel<Val>;
type Pcs = TwoAdicFriPcs<Val, Dft, ValMmcs, ChallengeMmcs>;

/// `StarkConfig` for the PQ-Semaphore AIR.
pub type Config = StarkConfig<Pcs, Challenge, Challenger>;
/// Concrete proof type for this AIR.
pub type Proof = p3_uni_stark::Proof<Config>;

// FRI parameters. 64 queries is the canonical 95-bit count for
// `BabyBear × Quartic × log_blowup=1`; the 16+17 grinding bits stack
// to 128 conjectured (Phase F: tighten to literal 128 classical).
const LOG_BLOWUP: usize = 1;
const NUM_QUERIES: usize = 64;
const COMMIT_POW_BITS: usize = 16;
const QUERY_POW_BITS: usize = 17;
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
    /// inputs `[0..4]` when `direction_bit == 0` else `[4..8]`.
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
    /// the audited `BABYBEAR_POSEIDON2_RC_16_*` constants.
    raw: RawRoundConstants,
}

// Note: `PqSemaphoreAir` is the AIR type; downstream callers can name it directly.

#[derive(Debug)]
struct RawRoundConstants {
    beginning_full: [[Val; WIDTH]; HALF_FULL_ROUNDS],
    partial: [Val; PARTIAL_ROUNDS],
    ending_full: [[Val; WIDTH]; HALF_FULL_ROUNDS],
}

/// Construct the AIR with audited Poseidon2-`BabyBear` round constants.
#[must_use]
pub const fn build_air() -> PqSemaphoreAir {
    let constants = RoundConstants::<Val, WIDTH, HALF_FULL_ROUNDS, PARTIAL_ROUNDS>::new(
        BABYBEAR_POSEIDON2_RC_16_EXTERNAL_INITIAL,
        BABYBEAR_POSEIDON2_RC_16_INTERNAL,
        BABYBEAR_POSEIDON2_RC_16_EXTERNAL_FINAL,
    );
    let raw = RawRoundConstants {
        beginning_full: BABYBEAR_POSEIDON2_RC_16_EXTERNAL_INITIAL,
        partial: BABYBEAR_POSEIDON2_RC_16_INTERNAL,
        ending_full: BABYBEAR_POSEIDON2_RC_16_EXTERNAL_FINAL,
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
        // We DO use next-row access (continuity / id-scope constancy).
        // Reporting all column indices is conservative; uni-stark uses
        // the list to know which columns to commit in the LDE for the
        // next row. Returning the full range is the simplest correct
        // answer; Plonky3 callers do this too when they don't bother
        // narrowing.
        (0..trace_width()).collect()
    }

    fn max_constraint_degree(&self) -> Option<usize> {
        // Let Plonky3's symbolic builder compute the actual degree.
        // Hand-rolling a hint only saves the symbolic walk and is not
        // worth the risk of missing a degree-pumping aux constraint.
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

        // 1. Audited Poseidon2 round constraints. Mirrors
        //    `Poseidon2Air::eval` from
        //    `vendor/Plonky3/poseidon2-air/src/air.rs:144-202`.
        eval_poseidon2::<AB>(&self.raw, builder, &local.poseidon2);

        // Copy public-input values out of the builder borrow so the
        // immutable borrow ends here, freeing `builder` for `assert_*`.
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

        // 3. Selectors mutually exclusive: exactly one of
        //    is_leaf / is_merkle_hop / is_nullifier / is_scope / is_padding == 1
        //    on every row. (is_root_check is a sub-selector of is_merkle_hop.)
        let one_active = local.is_leaf.into()
            + local.is_merkle_hop.into()
            + local.is_nullifier.into()
            + local.is_scope.into()
            + local.is_padding.into();
        builder.assert_eq(one_active, AB::Expr::ONE);

        // is_root_check ⇒ is_merkle_hop
        builder
            .assert_zero(local.is_root_check.into() * (AB::Expr::ONE - local.is_merkle_hop.into()));

        // 4. Capacity zeros on active rows (leaf / merkle / nullifier / scope).
        //    On Merkle rows the conditional swap fills inputs[0..2*DIGEST_WIDTH];
        //    the remaining capacity is always zero. On nullifier / scope rows
        //    we use inputs[0..DIGEST_WIDTH] = id/scope,
        //    [DIGEST_WIDTH..2*DIGEST_WIDTH] = scope/signal_hash,
        //    [2*DIGEST_WIDTH..WIDTH] = 0. On the leaf row we use
        //    inputs[0..DIGEST_WIDTH] = id, the rest = 0.
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

        // 6. Merkle hop conditional swap. For j in 0..DIGEST_WIDTH:
        //    inputs[j]               = prev_digest[j] * (1 - dir) + sibling[j] * dir
        //    inputs[j+DIGEST_WIDTH]  = prev_digest[j] * dir       + sibling[j] * (1 - dir)
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

        // 7. Nullifier row: inputs[0..DIGEST_WIDTH] = id,
        //    inputs[DIGEST_WIDTH..2*DIGEST_WIDTH] = scope.
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

        // 8. Scope-binding row: inputs[0..DIGEST_WIDTH] = scope,
        //    inputs[DIGEST_WIDTH..2*DIGEST_WIDTH] = signal_hash.
        for j in 0..DIGEST_WIDTH {
            builder.assert_zero(
                local.is_scope.into()
                    * (local.poseidon2.inputs[j].into() - local.scope_col[j].into()),
            );
            // signal_hash occupies public[2*DIGEST_WIDTH..3*DIGEST_WIDTH].
            let signal = public[2 * DIGEST_WIDTH + j].into();
            builder.assert_zero(
                local.is_scope.into() * (local.poseidon2.inputs[j + DIGEST_WIDTH].into() - signal),
            );
        }

        // 9. Public-input bindings on output (post[0..DIGEST_WIDTH] of the
        //    final full round) for each of the four "binding" rows.
        let final_full = HALF_FULL_ROUNDS - 1;
        for j in 0..DIGEST_WIDTH {
            let post = local.poseidon2.ending_full_rounds[final_full].post[j].into();

            // Merkle root: public[0..DIGEST_WIDTH], bound on is_root_check row.
            let root = public[j].into();
            builder.assert_zero(local.is_root_check.into() * (post.dup() - root));

            // Nullifier: public[DIGEST_WIDTH..2*DIGEST_WIDTH], bound on is_nullifier row.
            let nullifier = public[DIGEST_WIDTH + j].into();
            builder.assert_zero(local.is_nullifier.into() * (post.dup() - nullifier));

            // Scope hash: public[3*DIGEST_WIDTH..4*DIGEST_WIDTH], bound on is_scope row.
            let scope_hash = public[3 * DIGEST_WIDTH + j].into();
            builder.assert_zero(local.is_scope.into() * (post - scope_hash));
        }

        // 10. Inter-row continuity.
        //
        //   - id_col and scope_col are constant across the trace.
        //   - prev_digest of the next row = post[0..4] of the current row,
        //     when the next row is a Merkle hop (or, for the convention
        //     here, on every transition: the `next.prev_digest` is fed
        //     the current row's output digest, conditioned on
        //     `next.is_merkle_hop`).
        //
        // We use `when_transition` so the constraint doesn't reach
        // beyond the last row.
        let final_full = HALF_FULL_ROUNDS - 1;
        let mut t = builder.when_transition();
        for j in 0..DIGEST_WIDTH {
            // id_col / scope_col constancy.
            t.assert_eq(local.id_col[j], next.id_col[j]);
            t.assert_eq(local.scope_col[j], next.scope_col[j]);

            // prev_digest continuity, gated by next.is_merkle_hop.
            let cur_post = local.poseidon2.ending_full_rounds[final_full].post[j].into();
            t.assert_zero(next.is_merkle_hop.into() * (next.prev_digest[j].into() - cur_post));
        }
    }
}

// ============================================================================
// Build a `Config` (audited Poseidon2 perm; 64 queries; 95-bit FRI + 32 grinding).
// ============================================================================

/// Build a fresh `Config` from the audited Poseidon2-`BabyBear` permutation.
#[must_use]
pub fn make_config() -> Config {
    let perm = default_babybear_poseidon2_16();
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

/// Parse the 64-byte public-inputs blob into 16 canonical `BabyBear`
/// elements (little-endian).
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
    // BabyBear prime: 2^31 - 2^27 + 1 = 2_013_265_921 = 0x78000001.
    const BABYBEAR_PRIME: u32 = 0x7800_0001;
    for (i, chunk) in bytes.chunks_exact(4).enumerate() {
        let arr: [u8; 4] = chunk.try_into().map_err(|_| Error::PublicDeserialization)?;
        let v = u32::from_le_bytes(arr);
        if v >= BABYBEAR_PRIME {
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

/// Run the audited Poseidon2-`BabyBear` permutation and return the full
/// 16-element output state. Used by the host-side prover when populating
/// witness columns; verifier code does not call this.
pub fn poseidon2_permute(state: [Val; WIDTH]) -> [Val; WIDTH] {
    let perm = default_babybear_poseidon2_16();
    let mut out = state;
    perm.permute_mut(&mut out);
    out
}

// ============================================================================
// Witness / trace construction helpers (no_std; no `prove` dependency).
//
// These helpers let downstream prover crates (and tests in this crate)
// build a valid trace from a deterministic witness. The actual `prove`
// call lives in the host-gen crate or in `dev-dependencies` because
// Plonky3's prover requires `std`.
// ============================================================================

/// Witness for one PQ-Semaphore proof. Public so callers (host prover
/// or tests) can construct one directly.
#[derive(Clone, Debug)]
pub struct PqSemaphoreWitness {
    /// Identity commitment, 4 `BabyBear` elements.
    pub id: [Val; DIGEST_WIDTH],
    /// Application scope, 4 elements.
    pub scope: [Val; DIGEST_WIDTH],
    /// Pre-hashed message (we treat the message as already-hashed for v0).
    pub signal_hash: [Val; DIGEST_WIDTH],
    /// 10 sibling values along the Merkle path.
    pub siblings: [[Val; DIGEST_WIDTH]; TREE_DEPTH],
    /// 10 direction bits along the Merkle path.
    pub direction_bits: [bool; TREE_DEPTH],
    /// Computed leaf = H(id || 0^12).
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
/// tree" scenario. Used by the host prover and the tests.
///
/// `id`, `scope`, `signal_hash` are caller-supplied; the function builds
/// the dummy Merkle tree (leaves 1..1024 are `H((i, 0, 0, 0))`) and
/// returns the path siblings + computed root + nullifier + scope_hash.
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
                let i_u32 = i as u32;
                v[0] = Val::new(i_u32);
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

/// Encode a packed public-inputs array into the 64-byte little-endian wire
/// format consumed by [`parse_public_inputs`].
#[must_use]
pub fn encode_public_inputs(public: &[Val; NUM_PUBLIC_INPUTS]) -> Vec<u8> {
    use p3_field::PrimeField32;
    let mut out = Vec::with_capacity(PUBLIC_INPUTS_BYTES);
    for v in public {
        out.extend_from_slice(&v.as_canonical_u32().to_le_bytes());
    }
    out
}

/// Build the trace matrix for a given witness. Returns a flat
/// `Vec<Val>` of length `NUM_TRACE_ROWS * trace_width()`; callers wrap it
/// in `RowMajorMatrix::new(values, trace_width())` before calling
/// `prove`.
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
/// columns into `dst`. Mirrors Plonky3's `generate_trace_rows_for_perm`
/// but operates directly on `Val` (no `MaybeUninit` because we own a
/// fully-initialised `Vec<Val>`).
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
    type LL = GenericPoseidon2LinearLayersBabyBear;

    dst.inputs = state;

    LL::external_linear_layer(&mut state);

    for round_idx in 0..HALF_FULL_ROUNDS {
        let rc = &BABYBEAR_POSEIDON2_RC_16_EXTERNAL_INITIAL[round_idx];
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
        let rc = BABYBEAR_POSEIDON2_RC_16_INTERNAL[round_idx];
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
        let rc = &BABYBEAR_POSEIDON2_RC_16_EXTERNAL_FINAL[round_idx];
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
// Copied verbatim from `vendor/Plonky3/poseidon2-air/src/air.rs:144-323`
// because the helpers (`eval`, `eval_full_round`, `eval_partial_round`,
// `eval_sbox`) are `pub(crate)` and not callable from outside the
// `p3_poseidon2_air` crate. The audit at
// `crates/zkmcu-poseidon-audit` covers the constraint shape these
// helpers produce; copying preserves audit coverage as long as the
// bytes match. Vendor path is read-only by project policy.
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

    GenericPoseidon2LinearLayersBabyBear::external_linear_layer(&mut state);

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
    GenericPoseidon2LinearLayersBabyBear::external_linear_layer(state);
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

    GenericPoseidon2LinearLayersBabyBear::internal_linear_layer(state);
}

#[inline]
fn eval_sbox<AB>(
    sbox: &SBox<AB::Var, SBOX_DEGREE, SBOX_REGISTERS>,
    x: &mut AB::Expr,
    builder: &mut AB,
) where
    AB: AirBuilder,
{
    // Specialised for (DEGREE = 7, REGISTERS = 1).
    let committed_x3 = sbox.0[0].into();
    builder.assert_eq(committed_x3.dup(), x.cube());
    *x = committed_x3.square() * x.dup();
}
