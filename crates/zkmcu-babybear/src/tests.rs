//! Validation tests for the `BabyBear` base field.
//!
//! Strategy: compare each arithmetic op against a canonical-u64 reference (all
//! values < `M` < 2^31, so u64 multiplication never overflows). Also checks the
//! Montgomery invariants (round-trip through `new`/`canonical`), that `GENERATOR`
//! really is primitive, and that `TWO_ADIC_ROOT_OF_UNITY` has the claimed order.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation,
    clippy::cast_lossless,
    clippy::integer_division
)]

use alloc::vec::Vec;

use winter_utils::{Deserializable, Serializable, SliceReader};
use winterfell::math::fields::QuartExtension;
use winterfell::math::{ExtensibleField, FieldElement, StarkField};

use crate::field::{BaseElement, M};

const M64: u64 = M as u64;

fn ref_add(a: u32, b: u32) -> u32 {
    ((u64::from(a) + u64::from(b)) % M64) as u32
}

fn ref_sub(a: u32, b: u32) -> u32 {
    ((u64::from(a) + M64 - u64::from(b)) % M64) as u32
}

fn ref_mul(a: u32, b: u32) -> u32 {
    ((u64::from(a) * u64::from(b)) % M64) as u32
}

/// Deterministic xorshift, we want the same test inputs on every run.
struct Xor(u64);

impl Xor {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x as u32) % M
    }
}

#[test]
fn modulus_is_proth() {
    assert_eq!(M, 0x7800_0001);
    assert_eq!(M, 15 * (1u32 << 27) + 1);
    assert_eq!(BaseElement::MODULUS, u64::from(M));
    assert_eq!(BaseElement::MODULUS_BITS, 31);
    assert_eq!(BaseElement::TWO_ADICITY, 27);
}

#[test]
fn p_prime_is_correct() {
    // P_PRIME is private, we re-derive its property: p * q ≡ 1 (mod 2^32) with
    // q = 0x8800_0001, so -p^(-1) mod 2^32 = 2^32 - q = 0x77FF_FFFF.
    let p: u64 = u64::from(M);
    let q: u64 = 0x8800_0001;
    assert_eq!((p * q) & 0xFFFF_FFFF, 1);
}

#[test]
fn new_and_canonical_round_trip() {
    for v in [0, 1, 2, M - 2, M - 1, M / 2, 12_345_678] {
        assert_eq!(BaseElement::new(v).canonical(), v);
    }
    let mut rng = Xor::new(0x1234_5678_9ABC_DEF0);
    for _ in 0..2048 {
        let v = rng.next();
        assert_eq!(BaseElement::new(v).canonical(), v);
    }
}

#[test]
fn new_reduces_non_canonical_inputs() {
    assert_eq!(BaseElement::new(M).canonical(), 0);
    assert_eq!(BaseElement::new(M + 1).canonical(), 1);
    assert_eq!(BaseElement::new(u32::MAX).canonical(), u32::MAX % M);
}

#[test]
fn add_matches_reference() {
    let mut rng = Xor::new(42);
    for _ in 0..4096 {
        let a = rng.next();
        let b = rng.next();
        let got = (BaseElement::new(a) + BaseElement::new(b)).canonical();
        assert_eq!(got, ref_add(a, b), "add({a}, {b})");
    }
}

#[test]
fn sub_matches_reference() {
    let mut rng = Xor::new(123);
    for _ in 0..4096 {
        let a = rng.next();
        let b = rng.next();
        let got = (BaseElement::new(a) - BaseElement::new(b)).canonical();
        assert_eq!(got, ref_sub(a, b), "sub({a}, {b})");
    }
}

#[test]
fn mul_matches_reference() {
    let mut rng = Xor::new(7);
    for _ in 0..4096 {
        let a = rng.next();
        let b = rng.next();
        let got = (BaseElement::new(a) * BaseElement::new(b)).canonical();
        assert_eq!(got, ref_mul(a, b), "mul({a}, {b})");
    }
}

#[test]
fn neg_is_additive_inverse() {
    assert_eq!((-BaseElement::ZERO).canonical(), 0);
    let mut rng = Xor::new(99);
    for _ in 0..1024 {
        let x = BaseElement::new(rng.next());
        assert_eq!((x + (-x)).canonical(), 0);
    }
}

#[test]
fn mul_by_inverse_is_one() {
    let mut rng = Xor::new(0xDEAD_BEEF);
    for _ in 0..256 {
        let a = rng.next();
        if a == 0 {
            continue;
        }
        let x = BaseElement::new(a);
        let y = x.inv();
        assert_eq!((x * y).canonical(), 1, "a = {a}");
    }
}

#[test]
fn inv_of_zero_is_zero() {
    assert_eq!(BaseElement::ZERO.inv(), BaseElement::ZERO);
}

#[test]
fn distributivity() {
    let mut rng = Xor::new(1);
    for _ in 0..512 {
        let a = BaseElement::new(rng.next());
        let b = BaseElement::new(rng.next());
        let c = BaseElement::new(rng.next());
        assert_eq!(a * (b + c), a * b + a * c);
    }
}

#[test]
fn generator_is_primitive() {
    // p - 1 = 15 * 2^27 = 3 * 5 * 2^27, so primitivity needs non-triviality on
    // the three maximal proper subgroups: order (p-1)/2, (p-1)/3, (p-1)/5.
    let g = BaseElement::GENERATOR;
    let n = u64::from(M - 1);
    assert_ne!(g.exp(n / 2).canonical(), 1);
    assert_ne!(g.exp(n / 3).canonical(), 1);
    assert_ne!(g.exp(n / 5).canonical(), 1);
    // Sanity: full-order cycle comes back to 1.
    assert_eq!(g.exp(n).canonical(), 1);
}

#[test]
fn two_adic_root_of_unity_has_correct_order() {
    let omega = BaseElement::TWO_ADIC_ROOT_OF_UNITY;
    assert_eq!(omega.exp(1u64 << 27).canonical(), 1);
    assert_ne!(omega.exp(1u64 << 26).canonical(), 1);
}

#[test]
fn get_modulus_le_bytes_matches_constant() {
    let bytes = BaseElement::get_modulus_le_bytes();
    assert_eq!(bytes, M.to_le_bytes().to_vec());
}

#[test]
fn byte_round_trip() {
    let mut rng = Xor::new(0x0BAD_C0DE);
    for _ in 0..256 {
        let original = BaseElement::new(rng.next());
        let mut buf: Vec<u8> = Vec::new();
        original.write_into(&mut buf);
        assert_eq!(buf.len(), 4);
        let mut reader = SliceReader::new(&buf);
        let decoded = BaseElement::read_from(&mut reader).unwrap();
        assert_eq!(original, decoded);
    }
}

#[test]
fn deserialize_rejects_non_canonical() {
    let bad = M.to_le_bytes();
    let mut reader = SliceReader::new(&bad);
    assert!(BaseElement::read_from(&mut reader).is_err());
}

#[test]
fn try_from_bounds() {
    assert!(BaseElement::try_from(u64::from(M) - 1).is_ok());
    assert!(BaseElement::try_from(u64::from(M)).is_err());
    assert!(BaseElement::try_from(u128::from(M)).is_err());
}

#[test]
fn display_prints_canonical() {
    use alloc::format;
    let x = BaseElement::new(12345);
    assert_eq!(format!("{x}"), "12345");
    let y = BaseElement::new(M - 1);
    assert_eq!(format!("{y}"), "2013265920");
}

// QUARTIC EXTENSION TESTS
// ------------------------------------------------------------------------------------------------

fn rand_quartic(rng: &mut Xor) -> [BaseElement; 4] {
    [
        BaseElement::new(rng.next()),
        BaseElement::new(rng.next()),
        BaseElement::new(rng.next()),
        BaseElement::new(rng.next()),
    ]
}

#[test]
fn quartic_extension_is_supported() {
    assert!(<BaseElement as ExtensibleField<4>>::is_supported());
    assert!(QuartExtension::<BaseElement>::is_supported());
}

#[test]
fn quartic_mul_by_one_is_identity() {
    let mut rng = Xor::new(0x4001);
    let one = [
        BaseElement::ONE,
        BaseElement::ZERO,
        BaseElement::ZERO,
        BaseElement::ZERO,
    ];
    for _ in 0..128 {
        let a = rand_quartic(&mut rng);
        let got = <BaseElement as ExtensibleField<4>>::mul(a, one);
        assert_eq!(got, a);
    }
}

#[test]
fn quartic_mul_is_commutative() {
    let mut rng = Xor::new(0x4002);
    for _ in 0..128 {
        let a = rand_quartic(&mut rng);
        let b = rand_quartic(&mut rng);
        assert_eq!(
            <BaseElement as ExtensibleField<4>>::mul(a, b),
            <BaseElement as ExtensibleField<4>>::mul(b, a)
        );
    }
}

#[test]
fn quartic_mul_is_associative() {
    let mut rng = Xor::new(0x4003);
    for _ in 0..64 {
        let a = rand_quartic(&mut rng);
        let b = rand_quartic(&mut rng);
        let c = rand_quartic(&mut rng);
        let ab = <BaseElement as ExtensibleField<4>>::mul(a, b);
        let ab_c = <BaseElement as ExtensibleField<4>>::mul(ab, c);
        let bc = <BaseElement as ExtensibleField<4>>::mul(b, c);
        let a_bc = <BaseElement as ExtensibleField<4>>::mul(a, bc);
        assert_eq!(ab_c, a_bc);
    }
}

#[test]
fn quartic_distributivity() {
    let mut rng = Xor::new(0x4004);
    for _ in 0..64 {
        let a = rand_quartic(&mut rng);
        let b = rand_quartic(&mut rng);
        let c = rand_quartic(&mut rng);
        let b_plus_c = [b[0] + c[0], b[1] + c[1], b[2] + c[2], b[3] + c[3]];
        let lhs = <BaseElement as ExtensibleField<4>>::mul(a, b_plus_c);
        let ab = <BaseElement as ExtensibleField<4>>::mul(a, b);
        let ac = <BaseElement as ExtensibleField<4>>::mul(a, c);
        let rhs = [ab[0] + ac[0], ab[1] + ac[1], ab[2] + ac[2], ab[3] + ac[3]];
        assert_eq!(lhs, rhs);
    }
}

#[test]
fn quartic_mul_base_matches_mul() {
    let mut rng = Xor::new(0x4005);
    for _ in 0..128 {
        let a = rand_quartic(&mut rng);
        let b = BaseElement::new(rng.next());
        let b_ext = [b, BaseElement::ZERO, BaseElement::ZERO, BaseElement::ZERO];
        let via_mul = <BaseElement as ExtensibleField<4>>::mul(a, b_ext);
        let via_mul_base = <BaseElement as ExtensibleField<4>>::mul_base(a, b);
        assert_eq!(via_mul, via_mul_base);
    }
}

#[test]
fn quartic_frobenius_fixes_base_field() {
    let mut rng = Xor::new(0x4006);
    for _ in 0..128 {
        let b = BaseElement::new(rng.next());
        let embedded = [b, BaseElement::ZERO, BaseElement::ZERO, BaseElement::ZERO];
        let fr = <BaseElement as ExtensibleField<4>>::frobenius(embedded);
        assert_eq!(fr, embedded);
    }
}

#[test]
fn quartic_frobenius_fourth_iterate_is_identity() {
    // σ has order 4 in the Galois group of F_{p^4} / F_p, so σ^4 = identity.
    let mut rng = Xor::new(0x4007);
    for _ in 0..64 {
        let x = rand_quartic(&mut rng);
        let s1 = <BaseElement as ExtensibleField<4>>::frobenius(x);
        let s2 = <BaseElement as ExtensibleField<4>>::frobenius(s1);
        let s3 = <BaseElement as ExtensibleField<4>>::frobenius(s2);
        let s4 = <BaseElement as ExtensibleField<4>>::frobenius(s3);
        assert_eq!(s4, x);
    }
}

#[test]
fn quartic_frobenius_is_a_homomorphism() {
    // σ(a * b) = σ(a) * σ(b)
    let mut rng = Xor::new(0x4008);
    for _ in 0..64 {
        let a = rand_quartic(&mut rng);
        let b = rand_quartic(&mut rng);
        let ab = <BaseElement as ExtensibleField<4>>::mul(a, b);
        let fr_ab = <BaseElement as ExtensibleField<4>>::frobenius(ab);
        let sigma_a = <BaseElement as ExtensibleField<4>>::frobenius(a);
        let sigma_b = <BaseElement as ExtensibleField<4>>::frobenius(b);
        let product_of_sigmas = <BaseElement as ExtensibleField<4>>::mul(sigma_a, sigma_b);
        assert_eq!(fr_ab, product_of_sigmas);
    }
}

#[test]
fn quartic_norm_is_in_base_field() {
    // N(x) = x * σ(x) * σ²(x) * σ³(x) must lie in F_p (coefficients 1..=3 are zero).
    let mut rng = Xor::new(0x4009);
    for _ in 0..64 {
        let x = rand_quartic(&mut rng);
        let s1 = <BaseElement as ExtensibleField<4>>::frobenius(x);
        let s2 = <BaseElement as ExtensibleField<4>>::frobenius(s1);
        let s3 = <BaseElement as ExtensibleField<4>>::frobenius(s2);
        let x_s1 = <BaseElement as ExtensibleField<4>>::mul(x, s1);
        let s2_s3 = <BaseElement as ExtensibleField<4>>::mul(s2, s3);
        let norm = <BaseElement as ExtensibleField<4>>::mul(x_s1, s2_s3);
        assert_eq!(
            norm[1],
            BaseElement::ZERO,
            "norm coefficient 1 must be zero"
        );
        assert_eq!(
            norm[2],
            BaseElement::ZERO,
            "norm coefficient 2 must be zero"
        );
        assert_eq!(
            norm[3],
            BaseElement::ZERO,
            "norm coefficient 3 must be zero"
        );
    }
}

#[test]
fn quartic_inversion() {
    // Exercises the winterfell QuartExtension wrapper's FieldElement::inv implementation,
    // wich composes our ExtensibleField<4> primitives.
    let mut rng = Xor::new(0x400A);
    for _ in 0..64 {
        let coords = rand_quartic(&mut rng);
        if coords == [BaseElement::ZERO; 4] {
            continue;
        }
        let x = QuartExtension::new(coords[0], coords[1], coords[2], coords[3]);
        let x_inv = x.inv();
        assert_eq!(x * x_inv, QuartExtension::<BaseElement>::ONE);
    }
}

#[test]
fn elements_as_bytes_round_trip() {
    let mut rng = Xor::new(0xC0FF_EE01);
    let elements: Vec<BaseElement> = (0..16).map(|_| BaseElement::new(rng.next())).collect();
    let bytes = BaseElement::elements_as_bytes(&elements);
    assert_eq!(bytes.len(), elements.len() * 4);
    // Re-interpret as &[Self] via the unsafe method and check round-trip equality
    // (note: this verifies the repr, not canonical serialization).
    // SAFETY: the bytes were produced by elements_as_bytes from a valid &[Self].
    let back = unsafe { BaseElement::bytes_as_elements(bytes).unwrap() };
    assert_eq!(back, elements.as_slice());
}
