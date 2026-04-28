//! Subspace-trail condition verification for the internal layer.
//!
//! The Poseidon2 paper (Section 5.3, page 13-14) gives a sufficient
//! condition for `M_I` to prevent arbitrarily long subspace trails:
//! the minimal polynomials of `M_I, M_I², M_I³, ...` must all be
//! irreducible and of maximum degree.
//!
//! This module implements that check for `M_I` over `BabyBear` at
//! width 16, using Plonky3's published `V` vector. The audit verifies
//! the condition holds for `k = 1, ..., R_P = 13`.
//!
//! # Reduction to char poly irreducibility
//!
//! For an `n × n` matrix the minimal polynomial divides the
//! characteristic polynomial. If the characteristic polynomial is
//! irreducible, then it equals the minimal polynomial: degree `n`
//! (maximum) and irreducible. Conversely, if the characteristic
//! polynomial is reducible, the minimal polynomial cannot simultaneously
//! be irreducible and of degree `n`. So the paper's condition is
//! equivalent to: the characteristic polynomial of `M_I^k` is
//! irreducible.
//!
//! # Algorithms
//!
//! - **Char poly**: Faddeev-LeVerrier, `O(n⁴)` field ops, divides by
//!   small integers `1..=n` using Fermat-derived modular inverses.
//! - **Irreducibility**: Rabin's test. A monic `f` of degree `n` over
//!   `F_p` is irreducible iff `x^(p^n) ≡ x (mod f)` and
//!   `gcd(f, x^(p^(n/q)) - x) = 1` for every prime `q | n`.

#![allow(
    clippy::indexing_slicing,
    clippy::cast_possible_truncation,
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::needless_range_loop,
    clippy::integer_division
)]

use crate::internal_layer::{build_m_i_t16, T};
use crate::mds::BABYBEAR_P;
use crate::poly::{
    add_mod, inv_mod, is_one, mul_mod, poly_gcd, poly_mod, poly_mul_mod, poly_sub, poly_x, sub_mod,
    Poly,
};

/// `T × T` matrix over `F_p`, stored row-major. All audit arithmetic
/// uses raw `u64` lifted to `u128` internally so primes up to `2^64-1`
/// work without overflow.
pub type Matrix = [[u64; T]; T];

// ---- matrix operations -----------------------------------------------------

#[must_use]
pub const fn mat_zero() -> Matrix {
    [[0_u64; T]; T]
}

#[must_use]
pub const fn mat_identity() -> Matrix {
    let mut m = [[0_u64; T]; T];
    let mut i = 0;
    while i < T {
        m[i][i] = 1;
        i += 1;
    }
    m
}

#[must_use]
pub fn mat_add(a: &Matrix, b: &Matrix, p: u64) -> Matrix {
    let mut c = [[0_u64; T]; T];
    for i in 0..T {
        for j in 0..T {
            c[i][j] = add_mod(a[i][j], b[i][j], p);
        }
    }
    c
}

#[must_use]
pub fn mat_scalar_identity(c: u64, p: u64) -> Matrix {
    let mut m = [[0_u64; T]; T];
    let c = c % p;
    for i in 0..T {
        m[i][i] = c;
    }
    m
}

#[must_use]
pub fn mat_mul(a: &Matrix, b: &Matrix, p: u64) -> Matrix {
    let mut c = [[0_u64; T]; T];
    for i in 0..T {
        for j in 0..T {
            let mut acc: u64 = 0;
            for k in 0..T {
                acc = add_mod(acc, mul_mod(a[i][k], b[k][j], p), p);
            }
            c[i][j] = acc;
        }
    }
    c
}

#[must_use]
pub fn mat_trace(a: &Matrix, p: u64) -> u64 {
    let mut t: u64 = 0;
    for i in 0..T {
        t = add_mod(t, a[i][i], p);
    }
    t
}

/// `A^k` via repeated multiplication.
#[must_use]
pub fn mat_pow(a: &Matrix, k: u32, p: u64) -> Matrix {
    if k == 0 {
        return mat_identity();
    }
    let mut result = *a;
    for _ in 1..k {
        result = mat_mul(&result, a, p);
    }
    result
}

// ---- characteristic polynomial via Faddeev-LeVerrier -----------------------

/// Characteristic polynomial of `A`, returned as `[c_0, c_1, ..., c_{n-1}, 1]`
/// (low-order first) so the leading coefficient is at index `n`.
///
/// Algorithm: Faddeev-LeVerrier. `M_1 = A`, `c_{n-1} = -trace(M_1)`.
/// For `k = 2..=n`: `M_k = A · (M_{k-1} + c_{n-k+1} · I)`,
/// `c_{n-k} = -trace(M_k) / k`.
#[must_use]
pub fn char_poly(a: &Matrix, p: u64) -> Poly {
    let n = T;
    let mut c = vec![0_u64; n + 1];
    c[n] = 1;

    let mut m_prev: Matrix = *a;
    c[n - 1] = sub_mod(0, mat_trace(&m_prev, p), p);

    for k in 2..=n {
        let shifted = mat_add(&m_prev, &mat_scalar_identity(c[n - k + 1], p), p);
        let m_k = mat_mul(a, &shifted, p);
        let inv_k = inv_mod(k as u64, p);
        c[n - k] = mul_mod(sub_mod(0, mat_trace(&m_k, p), p), inv_k, p);
        m_prev = m_k;
    }

    c
}

// ---- Rabin's irreducibility test -------------------------------------------

/// Distinct prime factors of `n`, in ascending order. For `n = 16`
/// returns `[2]`.
fn distinct_prime_factors(n: usize) -> Vec<usize> {
    let mut factors = Vec::new();
    let mut x = n;
    let mut q = 2;
    while q * q <= x {
        if x % q == 0 {
            factors.push(q);
            while x % q == 0 {
                x /= q;
            }
        }
        q += 1;
    }
    if x > 1 {
        factors.push(x);
    }
    factors
}

/// `x^(p^k) mod f`, computed by raising `x` to the `p`-th power `k`
/// times in succession.
fn x_pow_p_k_mod(k: u32, f: &Poly, p: u64) -> Poly {
    let mut current = poly_mod(&poly_x(), f, p);
    for _ in 0..k {
        current = poly_pow_p_mod(&current, f, p);
    }
    current
}

/// `base^p mod f`, square-and-multiply with the prime `p` as exponent.
fn poly_pow_p_mod(base: &Poly, f: &Poly, p: u64) -> Poly {
    let mut result = vec![1_u64];
    let mut b = poly_mod(base, f, p);
    let mut e = p;
    while e > 0 {
        if e & 1 == 1 {
            result = poly_mul_mod(&result, &b, f, p);
        }
        b = poly_mul_mod(&b, &b, f, p);
        e >>= 1;
    }
    result
}

/// Rabin's irreducibility test for monic polynomial `f` of degree
/// `n ≥ 1` over `F_p`. Returns `true` iff `f` is irreducible.
#[must_use]
pub fn is_irreducible(f: &Poly, p: u64) -> bool {
    let n = match crate::poly::poly_degree(f) {
        Some(d) if d >= 1 => d,
        _ => return false,
    };
    if n == 1 {
        return true;
    }

    let x_poly = poly_x();

    // Step 1: x^(p^n) ≡ x (mod f).
    let xn = x_pow_p_k_mod(n as u32, f, p);
    if xn != x_poly {
        return false;
    }

    // Step 2: gcd(f, x^(p^(n/q)) - x) = 1 for each prime q | n.
    for q in distinct_prime_factors(n) {
        let xnq = x_pow_p_k_mod((n / q) as u32, f, p);
        let diff = poly_sub(&xnq, &x_poly, p);
        let g = poly_gcd(f, &diff, p);
        if !is_one(&g) {
            return false;
        }
    }

    true
}

// ---- the audit verification ------------------------------------------------

/// Verify the Poseidon2 subspace-trail condition for `M_I` at the
/// chosen width.
///
/// For each `k = 1, ..., r_p`, the characteristic polynomial of
/// `M_I^k` must be irreducible over `F_p`. Equivalent (via the
/// divides-relationship) to the paper's "minimal polynomial of
/// `M_I^k` is irreducible and of maximum degree".
///
/// Returns the smallest `k` at wich the condition fails, or `Ok(())`
/// if it holds for every `k` in range.
///
/// # Errors
///
/// Returns `Err(k)` if the characteristic polynomial of `M_I^k` is
/// reducible over `F_p`. That would be a real audit finding.
pub fn verify_subspace_trail_for_t16(r_p: u32) -> Result<(), u32> {
    let m_i = build_m_i_t16();
    for k in 1..=r_p {
        let m_k = mat_pow(&m_i, k, BABYBEAR_P);
        let chi = char_poly(&m_k, BABYBEAR_P);
        if !is_irreducible(&chi, BABYBEAR_P) {
            return Err(k);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::panic, clippy::doc_markdown, clippy::integer_division)]
    use super::*;
    use crate::params::INTERNAL_ROUNDS;

    // ---- matrix sanity ------------------------------------------------------

    #[test]
    fn mat_identity_is_idempotent() {
        let i = mat_identity();
        let ii = mat_mul(&i, &i, BABYBEAR_P);
        assert_eq!(ii, i);
    }

    #[test]
    fn mat_pow_zero_is_identity() {
        let m = build_m_i_t16();
        let result = mat_pow(&m, 0, BABYBEAR_P);
        assert_eq!(result, mat_identity());
    }

    #[test]
    fn mat_pow_two_matches_squaring() {
        let m = build_m_i_t16();
        let direct = mat_mul(&m, &m, BABYBEAR_P);
        let via_pow = mat_pow(&m, 2, BABYBEAR_P);
        assert_eq!(direct, via_pow);
    }

    // ---- Faddeev-LeVerrier sanity: a known small case (lifted to t=16
    // by padding) is hard to spot-check, so we instead verify the
    // *Cayley-Hamilton* identity: char_poly(A) evaluated at A is the
    // zero matrix.

    #[test]
    fn cayley_hamilton_holds_for_m_i() {
        let m = build_m_i_t16();
        let chi = char_poly(&m, BABYBEAR_P);
        // Evaluate chi(A) = c_0 I + c_1 A + c_2 A² + ... + c_n A^n.
        let mut acc = mat_zero();
        let mut a_pow = mat_identity();
        for (k, &coef) in chi.iter().enumerate() {
            // term = coef * A^k
            let mut term = [[0_u64; T]; T];
            for i in 0..T {
                for j in 0..T {
                    term[i][j] = mul_mod(coef, a_pow[i][j], BABYBEAR_P);
                }
            }
            acc = mat_add(&acc, &term, BABYBEAR_P);
            if k + 1 < chi.len() {
                a_pow = mat_mul(&a_pow, &m, BABYBEAR_P);
            }
        }
        assert_eq!(acc, mat_zero(), "Cayley-Hamilton must hold for M_I");
    }

    // ---- Rabin sanity: known irreducible / reducible cases over BabyBear.

    #[test]
    fn rabin_recognizes_reducible_quadratic() {
        // (x - 1)(x - 2) = x² - 3x + 2  in F_p
        let p = BABYBEAR_P;
        let f = vec![2_u64, sub_mod(0, 3, p), 1];
        assert!(!is_irreducible(&f, p));
    }

    #[test]
    fn rabin_recognizes_an_irreducible_quadratic_via_nonresidue() {
        // x² - c is irreducible over F_p iff c is a quadratic
        // non-residue. Find the smallest such c, then test.
        let p = BABYBEAR_P;
        let half = (p - 1) / 2;
        let mut nr: u64 = 0;
        for cand in 2..100u64 {
            if crate::poly::pow_mod(cand, half, p) == p - 1 {
                nr = cand;
                break;
            }
        }
        assert!(nr > 0, "should find a small non-residue mod BabyBear");
        let f = vec![sub_mod(0, nr, p), 0, 1];
        assert!(is_irreducible(&f, p));
    }

    #[test]
    fn rabin_recognizes_factorable_quartic() {
        // (x² + 1)(x² + 2) = x⁴ + 3x² + 2. Reducible regardless of
        // residue questions.
        let p = BABYBEAR_P;
        let f = vec![2_u64, 0, 3, 0, 1];
        assert!(!is_irreducible(&f, p));
    }

    // ---- the load-bearing audit assertion -----------------------------------

    /// For each `k = 1, ..., R_P = 13`, the characteristic polynomial
    /// of `M_I^k` must be irreducible over `BabyBear`. Equivalent to
    /// the Poseidon2 paper's subspace-trail condition. If this fails,
    /// Plonky3's chosen V vector does NOT satisfy the sufficient
    /// condition for arbitrary-trail prevention, wich would be a real
    /// audit finding.
    #[test]
    fn m_i_satisfies_subspace_trail_condition_for_all_k() {
        let result = verify_subspace_trail_for_t16(INTERNAL_ROUNDS as u32);
        match result {
            Ok(()) => {}
            Err(k) => panic!("char poly of M_I^{k} is REDUCIBLE over BabyBear"),
        }
    }
}
