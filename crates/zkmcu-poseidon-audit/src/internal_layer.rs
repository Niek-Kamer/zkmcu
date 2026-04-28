//! Construction of `M_I = J + diag(V)` for `BabyBear` Poseidon2 width 16.
//!
//! Plonky3's `BabyBearInternalLayerParameters::internal_layer_mat_mul`
//! (`vendor/Plonky3/baby-bear/src/poseidon2.rs:381`) defines the diagonal
//! vector as
//!
//! ```text
//! V = [-2, 1, 2, 1/2, 3, 4, -1/2, -3, -4, 1/2^8, 1/4, 1/8, 1/2^27, -1/2^8, -1/16, -1/2^27]
//! ```
//!
//! Where each fractional `1/2^k` is the modular inverse of `2^k` in
//! `F_p` for `p = 2_013_265_921`. The matrix is then `M_I[i][i] = V[i] + 1`,
//! `M_I[i][j] = 1` for `i ≠ j` (Section 5.2 of the Poseidon2 paper).
//!
//! This module reconstructs `V` from first principles (Plonky3's halve /
//! `div_2exp_u64` ops, expressed as plain modular arithmetic) and builds
//! `M_I`. Slice 3b will then verify the subspace-trail condition by
//! computing `M_I^k` and the minimal polynomial of each, checking each
//! is irreducible and of maximum degree (Section 5.3 page 13-14).

#![allow(
    clippy::indexing_slicing,
    clippy::integer_division,
    clippy::cast_possible_truncation
)]

use crate::mds::BABYBEAR_P;

/// Width of the `BabyBear` Poseidon2 instance audited here.
pub const T: usize = 16;

/// Modular halving over `F_p` with odd `p`. For even `x`, returns `x / 2`.
/// For odd `x`, returns `(x + p) / 2`, the unique value `y` with `2y = x`
/// in `F_p`.
const fn halve(x: u64, p: u64) -> u64 {
    if x % 2 == 0 {
        x / 2
    } else {
        (x + p) / 2
    }
}

/// `2^(-k) mod p`, computed as `k` successive halvings of `1`.
const fn inv_pow_2(k: u32, p: u64) -> u64 {
    let mut x: u64 = 1;
    let mut i: u32 = 0;
    while i < k {
        x = halve(x, p);
        i += 1;
    }
    x
}

const fn neg(x: u64, p: u64) -> u64 {
    if x == 0 {
        0
    } else {
        p - x
    }
}

/// Plonky3's V vector for `BabyBear` Poseidon2 width 16, lifted to
/// `u64` representations in `[0, BABYBEAR_P)`.
#[must_use]
pub const fn v_vector_t16() -> [u64; T] {
    let p = BABYBEAR_P;
    [
        neg(2, p),                //  V[0]  = -2
        1,                        //  V[1]  =  1
        2,                        //  V[2]  =  2
        inv_pow_2(1, p),          //  V[3]  =  1/2
        3,                        //  V[4]  =  3
        4,                        //  V[5]  =  4
        neg(inv_pow_2(1, p), p),  //  V[6]  = -1/2
        neg(3, p),                //  V[7]  = -3
        neg(4, p),                //  V[8]  = -4
        inv_pow_2(8, p),          //  V[9]  =  1/2^8
        inv_pow_2(2, p),          //  V[10] =  1/4
        inv_pow_2(3, p),          //  V[11] =  1/8
        inv_pow_2(27, p),         //  V[12] =  1/2^27
        neg(inv_pow_2(8, p), p),  //  V[13] = -1/2^8
        neg(inv_pow_2(4, p), p),  //  V[14] = -1/16  =  -1/2^4
        neg(inv_pow_2(27, p), p), //  V[15] = -1/2^27
    ]
}

/// Build the internal-layer matrix `M_I = J + diag(V)`, where `J` is the
/// all-ones `T × T` matrix. Equivalent to: `M_I[i][i] = V[i] + 1 mod p`,
/// `M_I[i][j] = 1` for `i ≠ j`.
#[must_use]
pub const fn build_m_i_t16() -> [[u64; T]; T] {
    let v = v_vector_t16();
    let p = BABYBEAR_P;
    let mut m = [[1_u64; T]; T];
    let mut i = 0;
    while i < T {
        m[i][i] = (v[i] + 1) % p;
        i += 1;
    }
    m
}

#[cfg(test)]
mod tests {
    #![allow(clippy::panic, clippy::doc_markdown, clippy::needless_range_loop)]
    use super::*;

    #[test]
    fn v_vector_has_correct_length() {
        let v = v_vector_t16();
        assert_eq!(v.len(), 16);
    }

    /// Direct-value spot checks against Plonky3's published V comment.
    #[test]
    fn v_vector_known_integer_entries() {
        let v = v_vector_t16();
        let p = BABYBEAR_P;
        assert_eq!(v[0], p - 2, "V[0] = -2");
        assert_eq!(v[1], 1);
        assert_eq!(v[2], 2);
        assert_eq!(v[4], 3);
        assert_eq!(v[5], 4);
        assert_eq!(v[7], p - 3, "V[7] = -3");
        assert_eq!(v[8], p - 4, "V[8] = -4");
    }

    /// The fractional entries must satisfy `2^k · V[i] ≡ ±1 (mod p)`,
    /// regardless of how we computed them.
    #[test]
    fn v_vector_fractional_entries_satisfy_inversion() {
        let v = v_vector_t16();
        let p = BABYBEAR_P;
        let mul_mod = |a: u64, b: u64| ((u128::from(a) * u128::from(b)) % u128::from(p)) as u64;
        let pow_2 = |k: u32| -> u64 {
            let mut x: u64 = 1;
            for _ in 0..k {
                x = mul_mod(x, 2);
            }
            x
        };

        // V[3] = 1/2 ⇒ 2 · V[3] ≡ 1
        assert_eq!(mul_mod(2, v[3]), 1, "2 · V[3] should be 1");
        // V[6] = -1/2 ⇒ 2 · V[6] ≡ -1 ≡ p - 1
        assert_eq!(mul_mod(2, v[6]), p - 1, "2 · V[6] should be -1");
        // V[9] = 1/2^8 ⇒ 2^8 · V[9] ≡ 1
        assert_eq!(mul_mod(pow_2(8), v[9]), 1);
        // V[10] = 1/4 ⇒ 4 · V[10] ≡ 1
        assert_eq!(mul_mod(4, v[10]), 1);
        // V[11] = 1/8 ⇒ 8 · V[11] ≡ 1
        assert_eq!(mul_mod(8, v[11]), 1);
        // V[12] = 1/2^27 ⇒ 2^27 · V[12] ≡ 1
        assert_eq!(mul_mod(pow_2(27), v[12]), 1);
        // V[13] = -1/2^8 ⇒ 2^8 · V[13] ≡ -1
        assert_eq!(mul_mod(pow_2(8), v[13]), p - 1);
        // V[14] = -1/16 ⇒ 16 · V[14] ≡ -1
        assert_eq!(mul_mod(16, v[14]), p - 1);
        // V[15] = -1/2^27 ⇒ 2^27 · V[15] ≡ -1
        assert_eq!(mul_mod(pow_2(27), v[15]), p - 1);
    }

    /// All V entries are valid field elements, strictly less than p.
    #[test]
    fn v_vector_entries_are_in_range() {
        let v = v_vector_t16();
        for &entry in &v {
            assert!(entry < BABYBEAR_P);
        }
    }

    /// `M_I = J + diag(V)`: 1's everywhere off-diagonal, `V[i] + 1` on
    /// the diagonal.
    #[test]
    fn m_i_has_expected_shape() {
        let m = build_m_i_t16();
        let v = v_vector_t16();
        let p = BABYBEAR_P;
        for i in 0..T {
            for j in 0..T {
                if i == j {
                    assert_eq!(m[i][j], (v[i] + 1) % p, "diag at i={i}");
                } else {
                    assert_eq!(m[i][j], 1, "off-diag at ({i},{j})");
                }
            }
        }
    }

    /// `M_I` has no zero entries (a property of the Poseidon2 design).
    /// If any diagonal entry equals `p - 1` then `M_I[i][i] = 0`, wich
    /// would be a real finding.
    #[test]
    fn m_i_has_no_zero_entries() {
        let m = build_m_i_t16();
        for i in 0..T {
            for j in 0..T {
                assert_ne!(m[i][j], 0, "M_I[{i}][{j}] is zero");
            }
        }
    }
}
