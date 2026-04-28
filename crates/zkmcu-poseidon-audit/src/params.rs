//! Target Poseidon2-`BabyBear` parameters for the PQ-Semaphore milestone.
//!
//! Numerical values in this module are **targets to verify**, not ground
//! truth. They mirror Plonky3's published `BabyBear` Poseidon2 instance
//! (`vendor/Plonky3/baby-bear/src/poseidon2.rs`) and the sage parameter
//! script (`vendor/poseidon2/poseidon2_rust_params.sage`). The audit's job
//! is to re-derive them independently (via the sage script + the bounds
//! in the Poseidon papers) and confirm agreement before they get treated
//! as authoritative.
//!
//! # Variant: Poseidon2 (eprint 2023/323)
//!
//! Poseidon2 restructures Poseidon1's rounds into an *external + internal*
//! split. External rounds use a full t×t MDS matrix and apply the S-box
//! to every state slot; internal rounds use a cheap diagonal matrix `M_I`
//! and apply the S-box to a single slot. The internal layer is the main
//! optimization: in-circuit cost drops from `O(t²)` to `O(t)` per internal
//! round while preserving the same security guarantees.
//!
//! See `vendor/poseidon2/papers/Poseidon2-2023-323.pdf`.
//!
//! # Target shape (rationale)
//!
//! - **Field**: `BabyBear` (31-bit prime, two-adicity 27). Locked by the
//!   STARK side; not audited here.
//! - **State width `t = 16`**: rate `r = 2`, capacity `c = 14`. Capacity
//!   bits `c · log2(p) ≈ 14 · 31 = 434` exceeds the 128-bit security floor
//!   with very generous margin. **t=16 is the canonical compression-mode
//!   width for `BabyBear`** per the sage script (line 16 of
//!   `poseidon2_rust_params.sage` selects t=16 for compression, t=24 for
//!   sponge) and Plonky3's published instance.
//! - **S-box exponent `α = 7`**: smallest exponent with `gcd(α, p-1) = 1`
//!   that resists interpolation attacks better than `α = 5` on a 31-bit
//!   field. Locked by the field choice + Poseidon2 paper recommendations.
//! - **External rounds `R_F = 8`** (split as 4 initial + 4 final): standard
//!   Poseidon2 layout. Plonky3's `BABYBEAR_POSEIDON2_HALF_FULL_ROUNDS = 4`.
//! - **Internal rounds `R_P = 13`**: Plonky3's published target value
//!   (`BABYBEAR_POSEIDON2_PARTIAL_ROUNDS_16 = 13`). The audit re-derives
//!   this from the brute-force search over the cryptanalytic bounds in
//!   `find_FD_round_numbers` (sage script lines 58-82) and confirms it
//!   matches.

use zkmcu_babybear::BaseElement;

/// State width of the sponge / compression function. `t = 16` is the
/// canonical compression-mode width for `BabyBear` Poseidon2.
pub const STATE_WIDTH: usize = 16;

/// Sponge rate: number of state slots that absorb input per permutation.
/// Set to 2 for 2-to-1 Merkle hashing.
pub const RATE: usize = 2;

/// Sponge capacity: state slots reserved for the security parameter.
/// `14 · 31 = 434` capacity bits, far above the 128-bit security floor.
pub const CAPACITY: usize = STATE_WIDTH - RATE;

/// S-box exponent. `x -> x^7` over `BabyBear`.
pub const ALPHA: u64 = 7;

/// Number of external rounds per half (initial half + final half).
///
/// Each external round applies the S-box to every state slot and a full
/// t×t MDS matrix multiplication. Plonky3 calls this `HALF_FULL_ROUNDS`.
pub const HALF_EXTERNAL_ROUNDS: usize = 4;

/// Total external rounds. Equivalent to `R_F` in Poseidon1 nomenclature.
pub const EXTERNAL_ROUNDS: usize = 2 * HALF_EXTERNAL_ROUNDS;

/// Number of internal rounds. Each internal round applies the S-box to a
/// single state slot and the cheap diagonal matrix `M_I`. Equivalent to
/// `R_P` in Poseidon1 nomenclature.
///
/// **Audit status**: target value is Plonky3's published `13`, pending
/// independent re-derivation via `poseidon2_rust_params.sage`.
pub const INTERNAL_ROUNDS: usize = 13;

/// Total round count, external + internal.
pub const TOTAL_ROUNDS: usize = EXTERNAL_ROUNDS + INTERNAL_ROUNDS;

/// One row of the external MDS matrix. The full matrix is `STATE_WIDTH`
/// of these. Concrete matrix lands in `mds.rs` once we pick (and prove)
/// one over `BabyBear` at width 16.
pub type ExternalMdsRow = [BaseElement; STATE_WIDTH];

/// Internal layer matrix descriptor: the `μ` vector defining
/// `M_I = J + diag(μ - 1)`, where `J` is the all-ones matrix.
///
/// The matrix has 1's everywhere off-diagonal and `μ_i` on the diagonal
/// (Section 5.2 of the Poseidon2 paper, "Neptune-style"). Computed
/// efficiently as `output[i] = (μ_i - 1) · x_i + Σ x` with one shared
/// sum and one multiplication per output, so cost is `O(STATE_WIDTH)`
/// per internal round instead of `O(STATE_WIDTH²)`.
///
/// Storage convention (whether elements are `μ` or `μ - 1`) is settled
/// when the reference perm lands; at that point this type alias may
/// become a struct with a named field.
pub type InternalDiagonal = [BaseElement; STATE_WIDTH];

/// Round constants for one external round (one constant per state slot).
pub type ExternalRoundConstants = [BaseElement; STATE_WIDTH];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shape_is_consistent() {
        assert_eq!(CAPACITY + RATE, STATE_WIDTH);
        assert_eq!(EXTERNAL_ROUNDS, 2 * HALF_EXTERNAL_ROUNDS);
        assert_eq!(TOTAL_ROUNDS, EXTERNAL_ROUNDS + INTERNAL_ROUNDS);
    }
}
