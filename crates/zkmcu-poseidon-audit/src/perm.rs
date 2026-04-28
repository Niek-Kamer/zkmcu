//! Reference Poseidon2 permutation over `BabyBear`, width 16.
//!
//! Pure `u64` arithmetic with explicit `mod p` reductions, structured
//! to match the paper's Section 6 specification literally:
//!
//! ```text
//! P_2(x) = E_{R_F-1} ∘ ... ∘ E_{R_F/2} ∘ I_{R_P-1} ∘ ... ∘ I_0
//!         ∘ E_{R_F/2-1} ∘ ... ∘ E_0 ∘ M_E(x)
//! ```
//!
//! The leading `M_E · x` is the initial linear layer that fixes the
//! Bariant et al algebraic attack (Section 7.3, paper page 19-20).
//!
//! The constants are passed in by the caller, not hard-coded, so the
//! diff test (`tests/perm_diff.rs`) can drive both this impl and
//! Plonky3's with the same `BABYBEAR_POSEIDON2_RC_16_*` arrays and
//! confirm they produce byte-identical outputs.
//!
//! No external dependencies. Independent re-implementation of the spec.

#![allow(
    clippy::indexing_slicing,
    clippy::cast_possible_truncation,
    clippy::needless_range_loop,
    clippy::many_single_char_names
)]

use crate::internal_layer::{v_vector_t16, T};
use crate::mds::BABYBEAR_P;
use crate::params::{HALF_EXTERNAL_ROUNDS, INTERNAL_ROUNDS};
use crate::poly::{add_mod, mul_mod, pow_mod};

/// The 4×4 building-block matrix Plonky3 actually uses in production:
/// `circ(2, 3, 1, 1)`. The Poseidon2 paper's reference matrix is
/// `M_4 = circ(5, 7, 1, 3 / 4, 6, 1, 1 / ...)`; Plonky3 substitutes a
/// different MDS matrix that requires fewer additions (5 vs 8) per
/// row. Both are MDS over `BabyBear` (verified in `mds.rs`). This
/// deviation from the paper is intentional, documented in
/// `vendor/Plonky3/poseidon2/src/external.rs` near `apply_mat4`.
const M_4_PLONKY3: [[u64; 4]; 4] = [[2, 3, 1, 1], [1, 2, 3, 1], [1, 1, 2, 3], [3, 1, 1, 2]];

/// Build `M_E = circ(2·M_4, M_4, M_4, M_4)` for `t = 16` (Section 5.1
/// of the paper). Diagonal `4×4` blocks are `2·M_4`, off-diagonal
/// blocks are `M_4`.
#[must_use]
pub fn build_m_e(p: u64) -> [[u64; T]; T] {
    let mut m = [[0_u64; T]; T];
    for block_row in 0..4 {
        for block_col in 0..4 {
            let scale: u64 = if block_row == block_col { 2 } else { 1 };
            for i in 0..4 {
                for j in 0..4 {
                    m[block_row * 4 + i][block_col * 4 + j] = mul_mod(scale, M_4_PLONKY3[i][j], p);
                }
            }
        }
    }
    m
}

fn mat_vec_mul(m: &[[u64; T]; T], v: &[u64; T], p: u64) -> [u64; T] {
    let mut result = [0_u64; T];
    for i in 0..T {
        let mut acc: u64 = 0;
        for j in 0..T {
            acc = add_mod(acc, mul_mod(m[i][j], v[j], p), p);
        }
        result[i] = acc;
    }
    result
}

/// One external round: add `rc[i]` to each slot, apply `x → x^7`, then
/// multiply by `M_E`.
fn external_round(state: &mut [u64; T], rc: &[u64; T], m_e: &[[u64; T]; T], p: u64) {
    for i in 0..T {
        state[i] = add_mod(state[i], rc[i], p);
    }
    for i in 0..T {
        state[i] = pow_mod(state[i], 7, p);
    }
    *state = mat_vec_mul(m_e, state, p);
}

/// One internal round (Section 6, Remark 4): add `rc` to `state[0]`
/// only, apply `x → x^7` to `state[0]` only, then multiply by `M_I`
/// computed as `output[i] = V[i] · state[i] + Σ state` (cheap form).
fn internal_round(state: &mut [u64; T], rc: u64, v_vec: &[u64; T], p: u64) {
    state[0] = add_mod(state[0], rc, p);
    state[0] = pow_mod(state[0], 7, p);
    let mut sum: u64 = 0;
    for i in 0..T {
        sum = add_mod(sum, state[i], p);
    }
    for i in 0..T {
        state[i] = add_mod(mul_mod(v_vec[i], state[i], p), sum, p);
    }
}

/// Full Poseidon2 permutation for `BabyBear`, width 16. Applies the
/// initial `M_E`, then 4 initial external rounds, 13 internal rounds,
/// 4 final external rounds.
///
/// Constants are in canonical `[0, p)` form. The caller is responsible
/// for converting from any field representation (Plonky3's Montgomery
/// form, etc.) before calling.
pub fn poseidon2_permute_t16(
    state: &mut [u64; T],
    external_initial: &[[u64; T]; HALF_EXTERNAL_ROUNDS],
    external_final: &[[u64; T]; HALF_EXTERNAL_ROUNDS],
    internal: &[u64; INTERNAL_ROUNDS],
    p: u64,
) {
    let m_e = build_m_e(p);
    let v_vec = v_vector_t16();

    // Initial M_E layer (Section 4 fix for the Bariant attack).
    *state = mat_vec_mul(&m_e, state, p);

    // First half: 4 external rounds with the initial constants.
    for rc in external_initial {
        external_round(state, rc, &m_e, p);
    }

    // 13 internal rounds with one round constant each.
    for &rc in internal {
        internal_round(state, rc, &v_vec, p);
    }

    // Second half: 4 external rounds with the final constants.
    for rc in external_final {
        external_round(state, rc, &m_e, p);
    }
}

/// Convenience wrapper that uses [`BABYBEAR_P`].
pub fn poseidon2_permute_t16_babybear(
    state: &mut [u64; T],
    external_initial: &[[u64; T]; HALF_EXTERNAL_ROUNDS],
    external_final: &[[u64; T]; HALF_EXTERNAL_ROUNDS],
    internal: &[u64; INTERNAL_ROUNDS],
) {
    poseidon2_permute_t16(
        state,
        external_initial,
        external_final,
        internal,
        BABYBEAR_P,
    );
}

#[cfg(test)]
mod tests {
    #![allow(clippy::panic, clippy::doc_markdown)]
    use super::*;

    #[test]
    fn m_e_diagonal_blocks_are_2_times_m_4() {
        let m_e = build_m_e(BABYBEAR_P);
        for i in 0..4 {
            for j in 0..4 {
                assert_eq!(m_e[i][j], mul_mod(2, M_4_PLONKY3[i][j], BABYBEAR_P));
                assert_eq!(m_e[4 + i][4 + j], mul_mod(2, M_4_PLONKY3[i][j], BABYBEAR_P));
            }
        }
    }

    #[test]
    fn m_e_off_diagonal_blocks_are_m_4() {
        let m_e = build_m_e(BABYBEAR_P);
        for i in 0..4 {
            for j in 0..4 {
                assert_eq!(m_e[i][4 + j], M_4_PLONKY3[i][j]);
                assert_eq!(m_e[4 + i][j], M_4_PLONKY3[i][j]);
                assert_eq!(m_e[8 + i][12 + j], M_4_PLONKY3[i][j]);
            }
        }
    }

    /// Determinism: same inputs and constants produce same outputs.
    #[test]
    fn perm_is_deterministic() {
        let mut state_a = [42_u64; T];
        let mut state_b = [42_u64; T];
        let zeros_ext = [[0_u64; T]; HALF_EXTERNAL_ROUNDS];
        let zeros_int = [0_u64; INTERNAL_ROUNDS];
        poseidon2_permute_t16_babybear(&mut state_a, &zeros_ext, &zeros_ext, &zeros_int);
        poseidon2_permute_t16_babybear(&mut state_b, &zeros_ext, &zeros_ext, &zeros_int);
        assert_eq!(state_a, state_b);
    }

    /// Different inputs produce different outputs (probabilistic, but
    /// any reasonable permutation must satisfy this for distinct
    /// inputs).
    #[test]
    fn perm_distinguishes_inputs() {
        let zeros_ext = [[0_u64; T]; HALF_EXTERNAL_ROUNDS];
        let zeros_int = [0_u64; INTERNAL_ROUNDS];

        let mut state_a = [0_u64; T];
        let mut state_b = [1_u64; T];
        let mut state_c = [0_u64; T];
        state_c[0] = 1;

        poseidon2_permute_t16_babybear(&mut state_a, &zeros_ext, &zeros_ext, &zeros_int);
        poseidon2_permute_t16_babybear(&mut state_b, &zeros_ext, &zeros_ext, &zeros_int);
        poseidon2_permute_t16_babybear(&mut state_c, &zeros_ext, &zeros_ext, &zeros_int);

        assert_ne!(state_a, state_b);
        assert_ne!(state_a, state_c);
        assert_ne!(state_b, state_c);
    }
}
