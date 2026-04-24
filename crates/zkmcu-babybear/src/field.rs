//! `BabyBear` base field in Montgomery form (R = 2^32).

use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::{
    fmt::{self, Debug, Display, Formatter},
    mem,
    ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign},
};

use winter_utils::{
    AsBytes, ByteReader, ByteWriter, Deserializable, DeserializationError, Randomizable,
    Serializable,
};
use winterfell::math::{ExtensibleField, FieldElement, StarkField};

/// Field modulus `p = 15 * 2^27 + 1 = 0x7800_0001`.
pub const M: u32 = 0x7800_0001;

/// `-p^(-1) mod 2^32`. Derivation in `tests::p_prime_is_correct`, wich computes
/// `p * 0x8800_0001 mod 2^32` and checks it equals 1, so `2^32 - 0x8800_0001 = 0x77FF_FFFF`.
const P_PRIME: u32 = 0x77FF_FFFF;

/// `R^2 mod p` where `R = 2^32`. Used for the initial conversion of a canonical
/// value into Montgomery form: `mont_form(x) = mont_reduce(x * R2)`.
#[allow(clippy::cast_lossless)] // `u32 as u64` is infallible widening; `u64::from` is not const before Rust 1.83.
#[allow(clippy::cast_possible_truncation)] // `(r * r) % M` fits in u32 by construction.
const R2: u32 = {
    let r = (1_u64 << 32) % M as u64;
    ((r * r) % M as u64) as u32
};

/// Byte length of the canonical big-endian encoding of an element.
const ELEMENT_BYTES: usize = mem::size_of::<u32>();

// BASE ELEMENT
// ================================================================================================

/// A `BabyBear` field element in Montgomery form.
///
/// The invariant is that the inner `u32` holds `x * R mod p` where `x` is the
/// canonical representative in `[0, p)` and `R = 2^32`.
#[derive(Copy, Clone, Default)]
#[repr(transparent)]
pub struct BaseElement(u32);

impl BaseElement {
    /// Canonical constructor: reduces `value` mod p, then places it in Montgomery form.
    #[allow(clippy::cast_lossless, clippy::cast_possible_truncation)] // see R2 note
    #[must_use]
    pub const fn new(value: u32) -> Self {
        let canonical = value % M;
        let t = (canonical as u64) * (R2 as u64);
        Self(mont_reduce(t))
    }

    /// Construct from an already-Montgomery-form raw value. The caller is responsible
    /// for `value < M`.
    #[must_use]
    pub const fn from_mont(value: u32) -> Self {
        Self(value)
    }

    /// Raw Montgomery-form inner value.
    #[must_use]
    pub const fn inner(self) -> u32 {
        self.0
    }

    /// Canonical representative in `[0, p)`.
    #[allow(clippy::cast_lossless)] // see R2 note
    #[must_use]
    pub const fn canonical(self) -> u32 {
        mont_reduce(self.0 as u64)
    }
}

/// CIOS-style Montgomery reduction for `R = 2^32`.
///
/// For the intended use case `t = x * y` with `x, y ∈ [0, p)` we have
/// `t < p² < 2^62`, so `t + m*p < p² + (2^32 - 1)*p < 2^64`, the shift
/// lands in `[0, 2p)`, and one conditional subtraction canonicalises.
#[inline]
#[allow(clippy::cast_lossless, clippy::cast_possible_truncation)] // see R2 note
const fn mont_reduce(t: u64) -> u32 {
    let t_lo = t as u32;
    let m = t_lo.wrapping_mul(P_PRIME);
    let u = ((t + (m as u64) * (M as u64)) >> 32) as u32;
    if u >= M {
        u - M
    } else {
        u
    }
}

/// Modular exponentiation on canonical u64 representatives. Used only to compute
/// `TWO_ADIC_ROOT_OF_UNITY` at compile time, not in the hot path.
#[allow(clippy::cast_lossless, clippy::cast_possible_truncation)] // see R2 note
const fn pow_mod_p(base: u32, mut exp: u64) -> u32 {
    let m = M as u64;
    let mut result = 1_u64;
    let mut b = (base as u64) % m;
    while exp > 0 {
        if exp & 1 == 1 {
            result = (result * b) % m;
        }
        exp >>= 1;
        b = (b * b) % m;
    }
    result as u32
}

/// `31^15 mod p`. Since `(p - 1) / 2^27 = 15` and `31` is a primitive element of `F_p*`,
/// this is a generator of the order-`2^27` multiplicative subgroup.
const TWO_ADIC_ROOT_OF_UNITY_INT: u32 = pow_mod_p(31, 15);

// QUADRATIC + CUBIC STUBS
// ================================================================================================
//
// Winterfell's `Air::BaseField` trait bound requires `ExtensibleField<2> + ExtensibleField<3> +
// ExtensibleField<4>`. For `BabyBear` only `<4>` is usable at 95-bit security (see `fibonacci_babybear`
// in `zkmcu-verifier-stark`), so the Quadratic and Cubic extensions are stubbed out with
// `is_supported() -> false`, matching the `f128` + `f62` + `f64` pattern in upstream winterfell for
// its own unsupported extensions. The runtime dispatch in `winter-prover` / `winter-verifier`
// surfaces `UnsupportedFieldExtension(2)` or `(3)` if you try to use them.

// `unimplemented!()` is idiomatic here (it matches the upstream winterfell stub pattern for
// unsupported extensions on f128 etc.) but our workspace lints warn on it. The methods are
// unreachable at runtime because `is_supported() -> false` is checked by `winter-prover` /
// `winter-verifier` before dispatch; hitting one would indicate a programmer error, not user
// input.
#[allow(clippy::unimplemented)]
impl ExtensibleField<2> for BaseElement {
    fn mul(_a: [Self; 2], _b: [Self; 2]) -> [Self; 2] {
        unimplemented!()
    }

    fn mul_base(_a: [Self; 2], _b: Self) -> [Self; 2] {
        unimplemented!()
    }

    fn frobenius(_x: [Self; 2]) -> [Self; 2] {
        unimplemented!()
    }

    fn is_supported() -> bool {
        false
    }
}

#[allow(clippy::unimplemented)]
impl ExtensibleField<3> for BaseElement {
    fn mul(_a: [Self; 3], _b: [Self; 3]) -> [Self; 3] {
        unimplemented!()
    }

    fn mul_base(_a: [Self; 3], _b: Self) -> [Self; 3] {
        unimplemented!()
    }

    fn frobenius(_x: [Self; 3]) -> [Self; 3] {
        unimplemented!()
    }

    fn is_supported() -> bool {
        false
    }
}

// QUARTIC EXTENSION
// ================================================================================================

/// Non-residue defining the irreducible polynomial `x^4 - W` for the quartic extension.
/// `11` is a quartic non-residue mod `p` (used by Plonky3's `BabyBear` extension too).
const QUARTIC_W: u32 = 11;

/// Frobenius twist coefficient `c = W^((p-1)/4) mod p`. Canonical form.
const FROB_C1: u32 = pow_mod_p(QUARTIC_W, (M as u64 - 1) >> 2);
/// `c^2 = W^((p-1)/2) mod p`. Since `(11/p) = -1` by quadratic reciprocity, this is `-1 = p - 1`.
const FROB_C2: u32 = M - 1;
/// `c^3 = c^2 * c = -c = p - c`. Uses `FROB_C1 > 0` wich holds because `11` has nonzero order.
const FROB_C3: u32 = M - FROB_C1;

/// Sparse multiplication by `W = 11 = 8 + 2 + 1`, saving a full Montgomery reduction.
/// `11·x = (x << 3) + (x << 1) + x` where each `<<` is implemented as repeated doubling
/// wich already canonicalises mod `p` via `Add`.
#[inline]
fn mul_by_w(x: BaseElement) -> BaseElement {
    let two_x = x.double();
    let eight_x = two_x.double().double();
    eight_x + two_x + x
}

impl ExtensibleField<4> for BaseElement {
    #[inline]
    fn mul(a: [Self; 4], b: [Self; 4]) -> [Self; 4] {
        // Karatsuba-style multiplication in `F_p[x] / (x^4 - W)`:
        // Split each factor into two degree-1 halves (low + high·φ²), compute three
        // degree-2 sub-products using 2-term Karatsuba (3 base mults each = 9 total),
        // then fold via `φ^4 = W`. Plus three sparse multiplies by `W = 11`.
        // Total: 9 base multiplications + 3 sparse-W multiplies + adds/subs.
        // Schoolbook was 16 base mults + 3 full-Mont `w * (...)` multiplies.

        // ---- L = A_lo * B_lo = a[0..2] * b[0..2], degree-2 poly [L0, L1, L2]
        let l0 = a[0] * b[0];
        let l2 = a[1] * b[1];
        let l1 = (a[0] + a[1]) * (b[0] + b[1]) - l0 - l2;

        // ---- H = A_hi * B_hi = a[2..4] * b[2..4], degree-2 poly [H0, H1, H2]
        let h0 = a[2] * b[2];
        let h2 = a[3] * b[3];
        let h1 = (a[2] + a[3]) * (b[2] + b[3]) - h0 - h2;

        // ---- T = (A_lo + A_hi) * (B_lo + B_hi), degree-2 poly [T0, T1, T2]
        let a_sum_0 = a[0] + a[2];
        let a_sum_1 = a[1] + a[3];
        let b_sum_0 = b[0] + b[2];
        let b_sum_1 = b[1] + b[3];
        let t0 = a_sum_0 * b_sum_0;
        let t2 = a_sum_1 * b_sum_1;
        let t1 = (a_sum_0 + a_sum_1) * (b_sum_0 + b_sum_1) - t0 - t2;

        // Mid = T - L - H
        let m0 = t0 - l0 - h0;
        let m1 = t1 - l1 - h1;
        let m2 = t2 - l2 - h2;

        // Combine, reducing φ^4 → W, φ^5 → Wφ, φ^6 → Wφ².
        //   c0 = L0 + W·(M2 + H0)
        //   c1 = L1 + W·H1
        //   c2 = L2 + M0 + W·H2
        //   c3 = M1
        let c0 = l0 + mul_by_w(m2 + h0);
        let c1 = l1 + mul_by_w(h1);
        let c2 = l2 + m0 + mul_by_w(h2);
        let c3 = m1;
        [c0, c1, c2, c3]
    }

    #[inline]
    fn mul_base(a: [Self; 4], b: Self) -> [Self; 4] {
        [a[0] * b, a[1] * b, a[2] * b, a[3] * b]
    }

    #[inline]
    fn frobenius(x: [Self; 4]) -> [Self; 4] {
        // σ: x → x^p acts as identity on F_p and sends φ → c·φ, so σ(φ^i) = c^i · φ^i.
        [
            x[0],
            x[1] * Self::new(FROB_C1),
            x[2] * Self::new(FROB_C2),
            x[3] * Self::new(FROB_C3),
        ]
    }
}

// FIELD ELEMENT
// ================================================================================================

impl FieldElement for BaseElement {
    type PositiveInteger = u64;
    type BaseField = Self;

    const EXTENSION_DEGREE: usize = 1;
    const ELEMENT_BYTES: usize = ELEMENT_BYTES;
    const IS_CANONICAL: bool = false;

    const ZERO: Self = Self::new(0);
    const ONE: Self = Self::new(1);

    fn inv(self) -> Self {
        if self.0 == 0 {
            return Self::ZERO;
        }
        // Fermat: x^(p-2) = x^-1 in F_p*. Default `exp_vartime` is fine for
        // a STARK verifier; inversion is rare on the hot path (batch-inverse
        // optimisations happen a layer up).
        self.exp_vartime(u64::from(M - 2))
    }

    fn conjugate(&self) -> Self {
        *self
    }

    fn base_element(&self, i: usize) -> Self::BaseField {
        assert!(i == 0, "base field index must be 0, but was {i}");
        *self
    }

    fn slice_as_base_elements(elements: &[Self]) -> &[Self::BaseField] {
        elements
    }

    fn slice_from_base_elements(elements: &[Self::BaseField]) -> &[Self] {
        elements
    }

    fn elements_as_bytes(elements: &[Self]) -> &[u8] {
        let p = elements.as_ptr();
        let len = elements.len() * Self::ELEMENT_BYTES;
        // SAFETY: `BaseElement` is `#[repr(transparent)]` over `u32`, so a slice of `N`
        // elements is exactly `4N` bytes with the same alignment. The returned lifetime
        // is tied to the input slice, no mutability shenanigans.
        unsafe { core::slice::from_raw_parts(p.cast::<u8>(), len) }
    }

    unsafe fn bytes_as_elements(bytes: &[u8]) -> Result<&[Self], DeserializationError> {
        if bytes.len() % Self::ELEMENT_BYTES != 0 {
            return Err(DeserializationError::InvalidValue(format!(
                "number of bytes ({}) does not divide into whole number of field elements",
                bytes.len(),
            )));
        }

        let p = bytes.as_ptr();
        // ELEMENT_BYTES is a non-zero compile-time constant; the modulo check above
        // guarantees exact division.
        #[allow(clippy::integer_division)]
        let len = bytes.len() / Self::ELEMENT_BYTES;

        if (p as usize) % mem::align_of::<u32>() != 0 {
            return Err(DeserializationError::InvalidValue(
                "slice memory alignment is not valid for this field element type".to_string(),
            ));
        }

        // SAFETY: the caller guarantees the bytes encode a valid sequence of Montgomery-form
        // BabyBear elements. The alignment and length checks above ensure the pointer is
        // well-typed for `&[Self]`. The `cast_ptr_alignment` allow is backed by the explicit
        // `% align_of::<u32>() == 0` check two lines up.
        #[allow(clippy::cast_ptr_alignment)]
        Ok(unsafe { core::slice::from_raw_parts(p.cast::<Self>(), len) })
    }
}

// STARK FIELD
// ================================================================================================

impl StarkField for BaseElement {
    const MODULUS: Self::PositiveInteger = M as u64;
    const MODULUS_BITS: u32 = 31;
    const GENERATOR: Self = Self::new(31);
    const TWO_ADICITY: u32 = 27;
    const TWO_ADIC_ROOT_OF_UNITY: Self = Self::new(TWO_ADIC_ROOT_OF_UNITY_INT);

    fn get_modulus_le_bytes() -> Vec<u8> {
        M.to_le_bytes().to_vec()
    }

    #[inline]
    fn as_int(&self) -> Self::PositiveInteger {
        u64::from(self.canonical())
    }
}

// RANDOMIZABLE / SERIALIZABLE / AS_BYTES
// ================================================================================================

impl Randomizable for BaseElement {
    const VALUE_SIZE: usize = ELEMENT_BYTES;

    fn from_random_bytes(bytes: &[u8]) -> Option<Self> {
        Self::try_from(bytes).ok()
    }
}

impl AsBytes for BaseElement {
    fn as_bytes(&self) -> &[u8] {
        let self_ptr: *const Self = self;
        // SAFETY: `#[repr(transparent)]` over `u32`, so a pointer to `Self` is a valid
        // pointer to `ELEMENT_BYTES` bytes with `u32` alignment.
        unsafe { core::slice::from_raw_parts(self_ptr.cast::<u8>(), ELEMENT_BYTES) }
    }
}

impl Serializable for BaseElement {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        target.write_bytes(&self.canonical().to_le_bytes());
    }

    fn get_size_hint(&self) -> usize {
        ELEMENT_BYTES
    }
}

impl Deserializable for BaseElement {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let value = source.read_u32()?;
        if value >= M {
            return Err(DeserializationError::InvalidValue(format!(
                "invalid field element: value {value} is greater than or equal to the field modulus"
            )));
        }
        Ok(Self::new(value))
    }
}

// EQUALITY
// ================================================================================================

impl PartialEq for BaseElement {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for BaseElement {}

// OVERLOADED OPERATORS
// ================================================================================================

impl Add for BaseElement {
    type Output = Self;

    #[inline]
    fn add(self, rhs: Self) -> Self {
        // `self.0`, `rhs.0 < M < 2^31`, so `self.0 + rhs.0 < 2^32`: no u32 overflow.
        let sum = self.0 + rhs.0;
        Self(if sum >= M { sum - M } else { sum })
    }
}

impl AddAssign for BaseElement {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl Sub for BaseElement {
    type Output = Self;

    #[inline]
    #[allow(clippy::suspicious_arithmetic_impl)] // two's-complement wrap used intentionally
    fn sub(self, rhs: Self) -> Self {
        let (diff, borrow) = self.0.overflowing_sub(rhs.0);
        Self(if borrow { diff.wrapping_add(M) } else { diff })
    }
}

impl SubAssign for BaseElement {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl Mul for BaseElement {
    type Output = Self;

    #[inline]
    fn mul(self, rhs: Self) -> Self {
        Self(mont_reduce(u64::from(self.0) * u64::from(rhs.0)))
    }
}

impl MulAssign for BaseElement {
    #[inline]
    fn mul_assign(&mut self, rhs: Self) {
        *self = *self * rhs;
    }
}

impl Div for BaseElement {
    type Output = Self;

    #[inline]
    #[allow(clippy::suspicious_arithmetic_impl)] // mul by inverse is the correct expansion
    fn div(self, rhs: Self) -> Self {
        self * rhs.inv()
    }
}

impl DivAssign for BaseElement {
    #[inline]
    fn div_assign(&mut self, rhs: Self) {
        *self = *self / rhs;
    }
}

impl Neg for BaseElement {
    type Output = Self;

    #[inline]
    fn neg(self) -> Self {
        if self.0 == 0 {
            self
        } else {
            Self(M - self.0)
        }
    }
}

// FORMATTING
// ================================================================================================

impl Debug for BaseElement {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{self}")
    }
}

impl Display for BaseElement {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.canonical())
    }
}

// CONVERSIONS INTO BaseElement
// ================================================================================================

impl From<bool> for BaseElement {
    fn from(value: bool) -> Self {
        Self::new(u32::from(value))
    }
}

impl From<u8> for BaseElement {
    fn from(value: u8) -> Self {
        Self::new(u32::from(value))
    }
}

impl From<u16> for BaseElement {
    fn from(value: u16) -> Self {
        Self::new(u32::from(value))
    }
}

impl From<u32> for BaseElement {
    fn from(value: u32) -> Self {
        Self::new(value)
    }
}

impl TryFrom<u64> for BaseElement {
    type Error = String;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        if value >= u64::from(M) {
            Err(format!(
                "invalid field element: value {value} is greater than or equal to the field modulus"
            ))
        } else {
            #[allow(clippy::cast_possible_truncation)] // value < M < 2^31, lossless
            Ok(Self::new(value as u32))
        }
    }
}

impl TryFrom<u128> for BaseElement {
    type Error = String;

    fn try_from(value: u128) -> Result<Self, Self::Error> {
        if value >= u128::from(M) {
            Err(format!(
                "invalid field element: value {value} is greater than or equal to the field modulus"
            ))
        } else {
            #[allow(clippy::cast_possible_truncation)] // value < M < 2^31, lossless
            Ok(Self::new(value as u32))
        }
    }
}

impl TryFrom<usize> for BaseElement {
    type Error = String;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        u64::try_from(value).map_or_else(
            |_| {
                Err(format!(
                    "invalid field element: value {value} does not fit in a u64"
                ))
            },
            Self::try_from,
        )
    }
}

impl TryFrom<[u8; 4]> for BaseElement {
    type Error = String;

    fn try_from(bytes: [u8; 4]) -> Result<Self, Self::Error> {
        let value = u32::from_le_bytes(bytes);
        if value >= M {
            Err(format!(
                "invalid field element: value {value} is greater than or equal to the field modulus"
            ))
        } else {
            Ok(Self::new(value))
        }
    }
}

impl TryFrom<&'_ [u8]> for BaseElement {
    type Error = DeserializationError;

    /// Parses a canonical little-endian encoding of a `BabyBear` element.
    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        let arr: [u8; ELEMENT_BYTES] = bytes.try_into().map_err(|_| {
            DeserializationError::InvalidValue(format!(
                "expected {ELEMENT_BYTES} bytes for a field element, got {}",
                bytes.len()
            ))
        })?;
        let value = u32::from_le_bytes(arr);
        if value >= M {
            return Err(DeserializationError::InvalidValue(format!(
                "invalid field element: value {value} is greater than or equal to the field modulus"
            )));
        }
        Ok(Self::new(value))
    }
}

// CONVERSIONS OUT OF BaseElement
// ================================================================================================

impl TryFrom<BaseElement> for bool {
    type Error = String;

    fn try_from(value: BaseElement) -> Result<Self, Self::Error> {
        match value.canonical() {
            0 => Ok(false),
            1 => Ok(true),
            v => Err(format!(
                "field element does not represent a boolean, got {v}"
            )),
        }
    }
}

impl TryFrom<BaseElement> for u8 {
    type Error = String;

    fn try_from(value: BaseElement) -> Result<Self, Self::Error> {
        value.canonical().try_into().map_err(|e| format!("{e}"))
    }
}

impl TryFrom<BaseElement> for u16 {
    type Error = String;

    fn try_from(value: BaseElement) -> Result<Self, Self::Error> {
        value.canonical().try_into().map_err(|e| format!("{e}"))
    }
}

impl From<BaseElement> for u32 {
    fn from(value: BaseElement) -> Self {
        value.canonical()
    }
}

impl From<BaseElement> for u64 {
    fn from(value: BaseElement) -> Self {
        Self::from(value.canonical())
    }
}

impl From<BaseElement> for u128 {
    fn from(value: BaseElement) -> Self {
        Self::from(value.canonical())
    }
}
