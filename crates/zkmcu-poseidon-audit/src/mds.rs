//! `M_4` MDS verification over `BabyBear`.
//!
//! Section 5.1 of the Poseidon2 paper claims `M_4` is MDS for primes
//! `p > 2^31`. `BabyBear`'s prime is `15 · 2^27 + 1 = 2_013_265_921`,
//! wich is **less than `2^31`**, so the paper's claim does not directly
//! cover us. Plonky3 ships `BABYBEAR_POSEIDON2_RC_16_*` constants that
//! assume `M_4` is MDS over `BabyBear` anyway. This module checks the
//! claim explicitly.
//!
//! # The check
//!
//! A `t × t` matrix is MDS iff every `k × k` submatrix is invertible
//! (non-zero determinant) for every `1 ≤ k ≤ t`. For `t = 4` that is
//! exactly **69 submatrices** (16 + 36 + 16 + 1 across `k = 1, 2, 3, 4`).
//! We enumerate all of them and verify each determinant is non-zero in
//! `F_p`. If any minor is zero, `M_4` is not MDS over `BabyBear` and
//! Plonky3's `BabyBear` Poseidon2 instance has a real problem.
//!
//! # Why raw `u64` arithmetic, not `BaseElement`
//!
//! The audit deliberately does not use `zkmcu_babybear::BaseElement`.
//! Validating `M_4` against the paper's claim is a question about the
//! prime `2_013_265_921` itself, not about our Montgomery-form field
//! impl. Using raw integer arithmetic with explicit `mod p` reductions
//! makes the audit independent of any bug in the field crate.

#![allow(
    clippy::indexing_slicing, // matrices, indices proven in-bounds by construction
    clippy::cast_possible_truncation,
)]

/// `BabyBear` modulus: `15 · 2^27 + 1 = 2_013_265_921`.
pub const BABYBEAR_P: u64 = 2_013_265_921;

/// The 4×4 building-block matrix `M_4` from Section 5.1 of the Poseidon2
/// paper. External layer matrix `M_E` for `t = 4·t'` is built as
/// `circ(2·M_4, M_4, ..., M_4)`.
pub const M_4: [[u64; 4]; 4] = [[5, 7, 1, 3], [4, 6, 1, 1], [1, 3, 5, 7], [1, 1, 4, 6]];

/// Description of a zero `k × k` minor: the row indices, column indices,
/// and `k` itself, so an auditor can re-extract the singular submatrix
/// from `M_4` by hand.
#[derive(Debug, Clone)]
pub struct MdsFailure {
    pub size: usize,
    pub rows: Vec<usize>,
    pub cols: Vec<usize>,
}

// Arithmetic uses `u128` internally so the same routines work for primes
// up to `2^64 - 1` (e.g. Goldilocks in the cross-check test) without
// overflowing.

fn add_mod(a: u64, b: u64, p: u64) -> u64 {
    ((u128::from(a) + u128::from(b)) % u128::from(p)) as u64
}

fn sub_mod(a: u64, b: u64, p: u64) -> u64 {
    let p128 = u128::from(p);
    let a128 = u128::from(a) % p128;
    let b128 = u128::from(b) % p128;
    ((a128 + p128 - b128) % p128) as u64
}

fn mul_mod(a: u64, b: u64, p: u64) -> u64 {
    ((u128::from(a) * u128::from(b)) % u128::from(p)) as u64
}

/// Determinant of the submatrix `M[rows][cols]` over `F_p`, by recursive
/// cofactor expansion along the first row of the submatrix.
fn det_submatrix(m: &[[u64; 4]; 4], rows: &[usize], cols: &[usize], p: u64) -> u64 {
    debug_assert_eq!(rows.len(), cols.len());
    let n = rows.len();
    if n == 1 {
        return m[rows[0]][cols[0]];
    }
    let mut total: u64 = 0;
    let mut sign_pos = true;
    let rows_tail = &rows[1..];
    for k in 0..n {
        let sub_cols: Vec<usize> = cols
            .iter()
            .enumerate()
            .filter_map(|(idx, &c)| if idx == k { None } else { Some(c) })
            .collect();
        let cofactor = det_submatrix(m, rows_tail, &sub_cols, p);
        let term = mul_mod(m[rows[0]][cols[k]], cofactor, p);
        total = if sign_pos {
            add_mod(total, term, p)
        } else {
            sub_mod(total, term, p)
        };
        sign_pos = !sign_pos;
    }
    total
}

/// All `k`-element subsets of `{0, 1, ..., n-1}` in lexicographic order.
fn k_subsets(n: usize, k: usize) -> Vec<Vec<usize>> {
    let total: u32 = 1u32 << n;
    (0..total)
        .filter(|bm| bm.count_ones() as usize == k)
        .map(|bm| (0..n).filter(|i| (bm >> i) & 1 == 1).collect())
        .collect()
}

/// Returns `Ok(num_minors_checked)` if `m` is MDS over `F_p`, else
/// `Err(MdsFailure)` describing the first zero minor encountered.
///
/// For a 4×4 matrix, `Ok` always carries the value 69.
pub fn is_mds_4x4(m: &[[u64; 4]; 4], p: u64) -> Result<usize, MdsFailure> {
    let mut checked: usize = 0;
    for k in 1..=4 {
        for row_subset in k_subsets(4, k) {
            for col_subset in k_subsets(4, k) {
                let det = det_submatrix(m, &row_subset, &col_subset, p);
                checked += 1;
                if det == 0 {
                    return Err(MdsFailure {
                        size: k,
                        rows: row_subset,
                        cols: col_subset,
                    });
                }
            }
        }
    }
    Ok(checked)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::panic, clippy::doc_markdown)]
    use super::*;

    /// Load-bearing audit assertion: M_4 is MDS over `BabyBear`'s prime
    /// despite `p < 2^31`. If this fails, Plonky3's `BabyBear` Poseidon2
    /// is broken at the linear-layer level.
    #[test]
    fn m4_is_mds_over_babybear() {
        let result = is_mds_4x4(&M_4, BABYBEAR_P);
        match result {
            Ok(n) => {
                assert_eq!(n, 69, "must enumerate 16 + 36 + 16 + 1 = 69 minors");
            }
            Err(failure) => {
                panic!("M_4 is NOT MDS over BabyBear: {failure:?}");
            }
        }
    }

    /// Plonky3's actual production matrix: `circ(2, 3, 1, 1)`. Used by
    /// `Poseidon2BabyBear` instead of the paper's `M_4`. Must also be
    /// MDS over `BabyBear`, otherwise Plonky3's deviation from the
    /// paper would be unsound.
    #[test]
    fn plonky3_optimized_m4_is_mds_over_babybear() {
        let plonky3_m4: [[u64; 4]; 4] = [[2, 3, 1, 1], [1, 2, 3, 1], [1, 1, 2, 3], [3, 1, 1, 2]];
        let result = is_mds_4x4(&plonky3_m4, BABYBEAR_P);
        assert!(
            matches!(result, Ok(69)),
            "Plonky3's optimized 4×4 matrix must be MDS over BabyBear"
        );
    }

    /// Cross-check: M_4 is also MDS over a much larger prime (the paper's
    /// stated `p > 2^31` regime). Sanity check that our verifier doesn't
    /// false-positive.
    #[test]
    fn m4_is_mds_over_goldilocks() {
        let goldilocks: u64 = 0xFFFF_FFFF_0000_0001;
        let result = is_mds_4x4(&M_4, goldilocks);
        assert!(matches!(result, Ok(69)));
    }

    /// Negative test: a 4×4 with two identical rows is singular, our
    /// checker must reject it.
    #[test]
    fn detects_singular_matrix_with_duplicate_rows() {
        let bad: [[u64; 4]; 4] = [[1, 2, 3, 4], [1, 2, 3, 4], [5, 6, 7, 8], [9, 10, 11, 12]];
        let result = is_mds_4x4(&bad, BABYBEAR_P);
        assert!(result.is_err());
    }

    /// Negative test: the identity matrix has zero entries off-diagonal,
    /// so any 2×2 submatrix that picks an off-diagonal cell has a zero
    /// minor. Identity is NOT MDS for `n ≥ 2`.
    #[test]
    fn detects_identity_as_non_mds() {
        let identity: [[u64; 4]; 4] = [[1, 0, 0, 0], [0, 1, 0, 0], [0, 0, 1, 0], [0, 0, 0, 1]];
        let result = is_mds_4x4(&identity, BABYBEAR_P);
        assert!(result.is_err());
    }
}
