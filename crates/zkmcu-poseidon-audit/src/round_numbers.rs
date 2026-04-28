//! Independent re-derivation of Poseidon2 round counts.
//!
//! Faithful port of the `sat_inequiv_alpha` + `find_FD_round_numbers`
//! functions from `vendor/poseidon2/poseidon2_rust_params.sage`. Encodes:
//!
//! - The five round-count bounds from the original Poseidon paper
//!   (statistical, interpolation, Groebner-1/2/3) per Section 3.2 of
//!   `vendor/poseidon2/papers/Poseidon2-2023-323.pdf`.
//! - The 2023/537 binomial cost addition for the CICO Groebner attack.
//! - The +7.5% partial-round security margin and +2 full-round margin.
//!
//! Output is `(R_F, R_P)`, the smallest pair of round counts that
//! satisfies every bound for the requested `(p, t, alpha, kappa)` tuple.
//!
//! # Why this lives here
//!
//! Plonky3 publishes hardcoded round counts for `BabyBear` Poseidon2
//! (`vendor/Plonky3/baby-bear/src/poseidon2.rs`). The paper's reference
//! Table 1 covers `(n=31, t=16, d=5)` but `BabyBear`'s smallest valid
//! S-box exponent is `d=7` (since `gcd(5, p-1) = 5 ≠ 1`), so the table
//! does not directly cover our case. This module re-runs the derivation
//! for `(p=2_013_265_921, t=16, d=7, kappa=128)` and confirms the result
//! matches Plonky3's published `13`. If the numbers ever drift, that's
//! a real audit finding instead of a silent disagreement.

// `suboptimal_flops` would push us toward `mul_add`, but FMA gives a
// single-rounded result wich diverges at the last bit from the sage
// reference's sequential `mul` then `add`. We want bit-exact agreement,
// so we keep the explicit form.
#![allow(
    clippy::float_arithmetic,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::suboptimal_flops,
    clippy::similar_names
)]

/// `log2(binomial(n, k))` computed as a sum of per-term `log2` values, so
/// the running product never overflows.
fn binomial_log2(n: u64, k: u64) -> f64 {
    if k > n {
        return f64::NEG_INFINITY;
    }
    let k = k.min(n - k);
    let mut acc: f64 = 0.0;
    for i in 0..k {
        acc += ((n - i) as f64).log2() - ((i + 1) as f64).log2();
    }
    acc
}

/// Predicate: does `(r_f, r_p)` satisfy every cryptanalytic bound for a
/// Poseidon2-style permutation over `F_p` with `t` state cells, S-box
/// exponent `alpha`, at security level `m` bits?
///
/// Faithful port of `sat_inequiv_alpha` (sage script lines 84-104).
#[must_use]
pub fn sat_inequiv(p: u64, t: usize, r_f: u32, r_p: u32, alpha: u32, m: u32) -> bool {
    let field_size: u32 = u64::BITS - p.leading_zeros();
    let log_p: f64 = (p as f64).log2();

    let alpha_f = f64::from(alpha);
    let m_f = f64::from(m);
    let t_f = t as f64;
    let r_f_f = f64::from(r_f);
    let r_p_f = f64::from(r_p);
    let field_size_f = f64::from(field_size);

    let log2_alpha = alpha_f.log2();
    let log_alpha_2 = 2.0_f64.log(alpha_f);
    let log_alpha_t = t_f.log(alpha_f);

    // R_F_1: statistical bound.
    let r_f_1 = if m_f <= ((log_p - (alpha_f - 1.0) / 2.0).floor() * (t_f + 1.0)) {
        6.0_f64
    } else {
        10.0_f64
    };

    // R_F_2: interpolation bound.
    let r_f_2 = 1.0 + (log_alpha_2 * m_f.min(field_size_f)).ceil() + log_alpha_t.ceil() - r_p_f;

    // R_F_3..5: three Groebner-basis bounds.
    let r_f_3 = log_alpha_2 * m_f.min(log_p) - r_p_f;
    let r_f_4 = t_f - 1.0 + log_alpha_2 * (m_f / (t_f + 1.0)).min(log_p / 2.0) - r_p_f;
    let r_f_5 = (t_f - 2.0 + m_f / (2.0 * log2_alpha) - r_p_f) / (t_f - 1.0);

    let r_f_max = r_f_1
        .ceil()
        .max(r_f_2.ceil())
        .max(r_f_3.ceil())
        .max(r_f_4.ceil())
        .max(r_f_5.ceil());

    // 2023/537 binomial bound (CICO-style Groebner attack cost).
    let r_temp = (t_f / 3.0).floor();
    let over_f = (r_f_f - 1.0) * t_f + r_p_f + r_temp + r_temp * (r_f_f / 2.0) + r_p_f + alpha_f;
    let under_f = r_temp * (r_f_f / 2.0) + r_p_f + alpha_f;
    let over = over_f as u64;
    let under = under_f as u64;
    let mut binom_log = binomial_log2(over, under);
    if binom_log.is_infinite() {
        binom_log = m_f + 1.0;
    }
    let cost_gb4 = (2.0 * binom_log).ceil();

    (r_f_f >= r_f_max) && (cost_gb4 >= m_f)
}

/// Smallest `(R_F, R_P)` pair satisfying every cryptanalytic bound,
/// minimizing total S-box cost `t · R_F + R_P`.
///
/// Faithful port of `find_FD_round_numbers` (sage script lines 58-82).
/// `security_margin = true` adds +2 full rounds and +7.5% partial rounds
/// (the values used in Plonky3's published instances).
#[must_use]
pub fn find_round_numbers(
    p: u64,
    t: usize,
    alpha: u32,
    m: u32,
    security_margin: bool,
) -> (u32, u32) {
    let mut min_cost: u64 = u64::MAX;
    let mut best_r_f: u32 = 0;
    let mut best_r_p: u32 = 0;

    for r_p_t in 1..500u32 {
        for r_f_t in (4..100u32).step_by(2) {
            if sat_inequiv(p, t, r_f_t, r_p_t, alpha, m) {
                let (r_f, r_p) = if security_margin {
                    let r_p_margin = (f64::from(r_p_t) * 1.075).ceil() as u32;
                    (r_f_t + 2, r_p_margin)
                } else {
                    (r_f_t, r_p_t)
                };
                let cost = u64::from(t as u32) * u64::from(r_f) + u64::from(r_p);
                if cost < min_cost || (cost == min_cost && r_f < best_r_f) {
                    min_cost = cost;
                    best_r_f = r_f;
                    best_r_p = r_p;
                }
            }
        }
    }

    (best_r_f, best_r_p)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `BabyBear` modulus: `15 · 2^27 + 1 = 2_013_265_921`.
    const BABYBEAR_P: u64 = 2_013_265_921;

    /// `Goldilocks` modulus: `2^64 - 2^32 + 1`.
    const GOLDILOCKS_P: u64 = 0xFFFF_FFFF_0000_0001;

    /// Plonky3's published `BABYBEAR_POSEIDON2_PARTIAL_ROUNDS_16 = 13`.
    /// This is the load-bearing audit assertion for the milestone.
    #[test]
    fn babybear_t16_alpha7_kappa128_matches_plonky3() {
        let (r_f, r_p) = find_round_numbers(BABYBEAR_P, 16, 7, 128, true);
        assert_eq!(r_f, 8, "R_F should be 8 (4+4 split)");
        assert_eq!(
            r_p, 13,
            "R_P should match Plonky3's BABYBEAR_POSEIDON2_PARTIAL_ROUNDS_16"
        );
    }

    /// Plonky3's published `BABYBEAR_POSEIDON2_PARTIAL_ROUNDS_24 = 21`.
    #[test]
    fn babybear_t24_alpha7_kappa128_matches_plonky3() {
        let (r_f, r_p) = find_round_numbers(BABYBEAR_P, 24, 7, 128, true);
        assert_eq!(r_f, 8);
        assert_eq!(r_p, 21);
    }

    /// Plonky3's published `BABYBEAR_POSEIDON2_PARTIAL_ROUNDS_32 = 30`.
    #[test]
    fn babybear_t32_alpha7_kappa128_matches_plonky3() {
        let (r_f, r_p) = find_round_numbers(BABYBEAR_P, 32, 7, 128, true);
        assert_eq!(r_f, 8);
        assert_eq!(r_p, 30);
    }

    /// Paper Table 1 row `(n=64, t=8, d=7) → R_F=8, R_P=22`.
    #[test]
    fn goldilocks_t8_alpha7_kappa128_matches_paper_table1() {
        let (r_f, r_p) = find_round_numbers(GOLDILOCKS_P, 8, 7, 128, true);
        assert_eq!(r_f, 8);
        assert_eq!(r_p, 22);
    }
}
