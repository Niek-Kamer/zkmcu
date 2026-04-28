//! Polynomial arithmetic over `F_p`, just enough for the subspace-trail
//! audit to verify characteristic polynomial irreducibility.
//!
//! Polynomials are stored as `Vec<u64>` with index `i` holding the
//! coefficient of `x^i`. The constant `0` polynomial is `vec![0]`.
//! After every operation we strip trailing zero coefficients (except
//! the constant zero), so the leading coefficient is non-zero.
//!
//! All arithmetic is in `F_p` for an arbitrary odd prime `p`. Internal
//! multiplications use `u128` so the same routines handle primes up to
//! `2^64 - 1` without overflowing.

#![allow(
    clippy::indexing_slicing,
    clippy::cast_possible_truncation,
    clippy::many_single_char_names,
    clippy::option_if_let_else,
    clippy::similar_names
)]

/// Polynomial over `F_p`: `coefficients[i]` holds the coefficient of
/// `x^i`. The constant zero polynomial is `[0]`.
pub type Poly = Vec<u64>;

// ---- field helpers ---------------------------------------------------------

#[must_use]
pub fn add_mod(a: u64, b: u64, p: u64) -> u64 {
    ((u128::from(a) + u128::from(b)) % u128::from(p)) as u64
}

#[must_use]
pub fn sub_mod(a: u64, b: u64, p: u64) -> u64 {
    let p128 = u128::from(p);
    ((u128::from(a) + p128 - u128::from(b) % p128) % p128) as u64
}

#[must_use]
pub fn mul_mod(a: u64, b: u64, p: u64) -> u64 {
    ((u128::from(a) * u128::from(b)) % u128::from(p)) as u64
}

#[must_use]
pub fn pow_mod(base: u64, exp: u64, p: u64) -> u64 {
    let p128 = u128::from(p);
    let mut result: u128 = 1;
    let mut b: u128 = u128::from(base) % p128;
    let mut e = exp;
    while e > 0 {
        if e & 1 == 1 {
            result = (result * b) % p128;
        }
        b = (b * b) % p128;
        e >>= 1;
    }
    result as u64
}

/// Modular inverse via Fermat's little theorem: `x^(p-2) mod p`. Caller
/// is responsible for `x != 0`.
#[must_use]
pub fn inv_mod(x: u64, p: u64) -> u64 {
    pow_mod(x, p - 2, p)
}

// ---- poly basic ops --------------------------------------------------------

fn normalize(mut a: Poly) -> Poly {
    while a.len() > 1 && *a.last().unwrap_or(&0) == 0 {
        a.pop();
    }
    a
}

#[must_use]
pub fn poly_zero() -> Poly {
    vec![0]
}

#[must_use]
pub fn poly_one() -> Poly {
    vec![1]
}

/// The polynomial `x`.
#[must_use]
pub fn poly_x() -> Poly {
    vec![0, 1]
}

/// Degree of a normalized polynomial. Zero polynomial returns `None`.
#[must_use]
pub fn poly_degree(a: &Poly) -> Option<usize> {
    if a.len() == 1 && a[0] == 0 {
        None
    } else {
        Some(a.len() - 1)
    }
}

/// `True` iff `a` is the constant polynomial `1`.
#[must_use]
pub fn is_one(a: &Poly) -> bool {
    a.len() == 1 && a[0] == 1
}

#[must_use]
pub fn poly_add(a: &Poly, b: &Poly, p: u64) -> Poly {
    let n = a.len().max(b.len());
    let mut c = vec![0_u64; n];
    for i in 0..n {
        let ai = if i < a.len() { a[i] } else { 0 };
        let bi = if i < b.len() { b[i] } else { 0 };
        c[i] = add_mod(ai, bi, p);
    }
    normalize(c)
}

#[must_use]
pub fn poly_sub(a: &Poly, b: &Poly, p: u64) -> Poly {
    let n = a.len().max(b.len());
    let mut c = vec![0_u64; n];
    for i in 0..n {
        let ai = if i < a.len() { a[i] } else { 0 };
        let bi = if i < b.len() { b[i] } else { 0 };
        c[i] = sub_mod(ai, bi, p);
    }
    normalize(c)
}

#[must_use]
pub fn poly_scale(a: &Poly, c: u64, p: u64) -> Poly {
    if c == 0 {
        return poly_zero();
    }
    let scaled: Vec<u64> = a.iter().map(|&ai| mul_mod(ai, c, p)).collect();
    normalize(scaled)
}

#[must_use]
pub fn poly_mul(a: &Poly, b: &Poly, p: u64) -> Poly {
    if poly_degree(a).is_none() || poly_degree(b).is_none() {
        return poly_zero();
    }
    let mut c = vec![0_u64; a.len() + b.len() - 1];
    for (i, &ai) in a.iter().enumerate() {
        if ai == 0 {
            continue;
        }
        for (j, &bj) in b.iter().enumerate() {
            c[i + j] = add_mod(c[i + j], mul_mod(ai, bj, p), p);
        }
    }
    normalize(c)
}

/// Long division: returns `(quotient, remainder)` with `a = q · b + r`
/// and `deg(r) < deg(b)`. Panics if `b == 0`.
#[must_use]
pub fn poly_div_rem(a: &Poly, b: &Poly, p: u64) -> (Poly, Poly) {
    let b_deg = poly_degree(b).expect("division by zero polynomial");
    if poly_degree(a).is_none_or(|da| da < b_deg) {
        return (poly_zero(), a.clone());
    }
    let lead_b_inv = inv_mod(b[b_deg], p);
    let a_deg = a.len() - 1;
    let mut r = a.clone();
    let mut q = vec![0_u64; a_deg - b_deg + 1];
    while let Some(rd) = poly_degree(&r) {
        if rd < b_deg {
            break;
        }
        let lead_r = r[rd];
        let coeff = mul_mod(lead_r, lead_b_inv, p);
        let shift = rd - b_deg;
        q[shift] = coeff;
        for j in 0..=b_deg {
            r[shift + j] = sub_mod(r[shift + j], mul_mod(coeff, b[j], p), p);
        }
        r = normalize(r);
    }
    (normalize(q), r)
}

#[must_use]
pub fn poly_mod(a: &Poly, b: &Poly, p: u64) -> Poly {
    poly_div_rem(a, b, p).1
}

#[must_use]
pub fn poly_mul_mod(a: &Poly, b: &Poly, m: &Poly, p: u64) -> Poly {
    poly_mod(&poly_mul(a, b, p), m, p)
}

/// `base^exp mod m`, square-and-multiply with `u64`-sized exponent.
#[must_use]
pub fn poly_pow_mod(base: &Poly, exp: u64, m: &Poly, p: u64) -> Poly {
    let mut result = poly_one();
    let mut b = poly_mod(base, m, p);
    let mut e = exp;
    while e > 0 {
        if e & 1 == 1 {
            result = poly_mul_mod(&result, &b, m, p);
        }
        b = poly_mul_mod(&b, &b, m, p);
        e >>= 1;
    }
    result
}

/// Make `a` monic (leading coefficient `1`) by scaling by the inverse
/// of its leading coefficient. Zero polynomial is returned unchanged.
#[must_use]
pub fn poly_make_monic(a: &Poly, p: u64) -> Poly {
    match poly_degree(a) {
        None => poly_zero(),
        Some(d) => {
            let inv = inv_mod(a[d], p);
            poly_scale(a, inv, p)
        }
    }
}

/// Greatest common divisor in `F_p[x]` via Euclidean algorithm. Result
/// is normalized to monic.
#[must_use]
pub fn poly_gcd(a: &Poly, b: &Poly, p: u64) -> Poly {
    let mut x = a.clone();
    let mut y = b.clone();
    while poly_degree(&y).is_some() {
        let r = poly_mod(&x, &y, p);
        x = y;
        y = r;
    }
    poly_make_monic(&x, p)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::panic, clippy::doc_markdown)]
    use super::*;

    const P: u64 = 2_013_265_921; // BabyBear

    #[test]
    fn add_sub_round_trip() {
        let a = vec![1, 2, 3];
        let b = vec![4, 5];
        let s = poly_add(&a, &b, P);
        let back = poly_sub(&s, &b, P);
        assert_eq!(back, a);
    }

    #[test]
    fn mul_distributes_over_add() {
        let a = vec![1, 2];
        let b = vec![3, 4];
        let c = vec![5, 6];
        let lhs = poly_mul(&a, &poly_add(&b, &c, P), P);
        let rhs = poly_add(&poly_mul(&a, &b, P), &poly_mul(&a, &c, P), P);
        assert_eq!(lhs, rhs);
    }

    #[test]
    fn div_rem_satisfies_a_eq_qb_plus_r() {
        let a = vec![1, 2, 3, 4, 5]; // 1+2x+3x²+4x³+5x⁴
        let b = vec![6, 7, 1]; // 6+7x+x²
        let (q, r) = poly_div_rem(&a, &b, P);
        let qb = poly_mul(&q, &b, P);
        let recon = poly_add(&qb, &r, P);
        assert_eq!(recon, a);
        // deg(r) < deg(b) = 2
        assert!(poly_degree(&r).is_none_or(|d| d < 2));
    }

    #[test]
    fn pow_mod_small_case() {
        // Compute (1 + x)^3 mod (x² + 1) over F_p.
        // (1+x)^3 = 1 + 3x + 3x² + x³.
        // Reduce mod (x²+1): x² ≡ -1 ⇒ x³ = -x; replace.
        // = 1 + 3x + 3(-1) + (-x) = -2 + 2x.
        let base = vec![1, 1];
        let m = vec![1, 0, 1];
        let result = poly_pow_mod(&base, 3, &m, P);
        let expected = vec![sub_mod(0, 2, P), 2];
        assert_eq!(result, expected);
    }

    #[test]
    fn gcd_with_self_is_self_monic() {
        let a = vec![2, 4, 6]; // 2 + 4x + 6x²
        let g = poly_gcd(&a, &a, P);
        // monic version of `a` is (1/6)·a
        let expected = poly_make_monic(&a, P);
        assert_eq!(g, expected);
    }

    #[test]
    fn fermat_inverse_round_trip() {
        let x: u64 = 1_234_567_890;
        let inv = inv_mod(x, P);
        assert_eq!(mul_mod(x, inv, P), 1);
    }
}
