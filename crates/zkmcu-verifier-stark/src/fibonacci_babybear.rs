//! Fibonacci AIR over `BabyBear` with `FieldExtension::Quartic`.
//!
//! Phase 3.3 companion to [`crate::fibonacci`], wich is the Goldilocks +
//! Quadratic baseline. The constraint math is identical (2-column trace,
//! two degree-1 transition constraints, three boundary assertions), so the
//! only thing that changes is the base field type and the public-input
//! encoding width.
//!
//! Why `Quartic` and not `Quadratic`: `BabyBear` has a 31-bit modulus, so a
//! Quadratic extension tops out at ~62 bits wich cannot carry 95-bit
//! conjectured soundness (the out-of-domain sampling term `(m + k) /
//! |F_ext|` bounds it). Quartic is 124 bits, comfortably above the
//! Phase 3.2 target. `FieldExtension::Quartic` is an extension wich lives
//! in the `Niek-Kamer/winterfell` fork, see `vendor/winterfell/math/src/
//! field/extensions/quartic.rs` for the `QuartExtension<B>` wrapper and
//! `crates/zkmcu-babybear/src/field.rs` for the `ExtensibleField<4>` impl
//! (irreducible polynomial `x^4 - 11`).
//!
//! The public-input wire format drops from 8 bytes (one Goldilocks `u64`)
//! to 4 bytes (one `BabyBear` `u32`). See [`PUBLIC_SIZE`].

use alloc::{vec, vec::Vec};

use winterfell::crypto::hashers::Blake3_256;
use winterfell::crypto::{DefaultRandomCoin, MerkleTree};
use winterfell::math::{FieldElement, StarkField, ToElements};
use winterfell::{
    AcceptableOptions, Air, AirContext, Assertion, EvaluationFrame, Proof, ProofOptions, TraceInfo,
    TransitionConstraintDegree,
};
use zkmcu_babybear::BaseElement;

use crate::Error;

/// Serialised size of the public-input wire format: one `BabyBear` field
/// element as a 4-byte little-endian `u32`.
pub const PUBLIC_SIZE: usize = 4;

/// Public inputs for the Fibonacci AIR over `BabyBear`.
#[derive(Debug, Clone, Copy)]
pub struct PublicInputs {
    /// The claimed result, `Fib(2 * trace_length)` reduced mod `p = 15 * 2^27 + 1`.
    pub result: BaseElement,
}

impl ToElements<BaseElement> for PublicInputs {
    fn to_elements(&self) -> Vec<BaseElement> {
        vec![self.result]
    }
}

/// Algebraic-intermediate-representation for the Fibonacci sequence over `BabyBear`.
pub struct FibAir {
    context: AirContext<BaseElement>,
    result: BaseElement,
}

#[allow(clippy::panic, clippy::indexing_slicing)] // same reasoning as `fibonacci::FibAir`
impl Air for FibAir {
    type BaseField = BaseElement;
    type PublicInputs = PublicInputs;

    fn new(trace_info: TraceInfo, pub_inputs: PublicInputs, options: ProofOptions) -> Self {
        let degrees = vec![
            TransitionConstraintDegree::new(1),
            TransitionConstraintDegree::new(1),
        ];
        assert_eq!(
            2,
            trace_info.width(),
            "Fibonacci AIR expects a 2-column trace"
        );
        let context = AirContext::new(trace_info, degrees, 3, options);
        Self {
            context,
            result: pub_inputs.result,
        }
    }

    fn context(&self) -> &AirContext<Self::BaseField> {
        &self.context
    }

    fn evaluate_transition<E: FieldElement + From<Self::BaseField>>(
        &self,
        frame: &EvaluationFrame<E>,
        _periodic_values: &[E],
        result: &mut [E],
    ) {
        let current = frame.current();
        let next = frame.next();
        // s_{0, i+1} = s_{0, i} + s_{1, i}
        result[0] = next[0] - (current[0] + current[1]);
        // s_{1, i+1} = s_{1, i} + s_{0, i+1}
        result[1] = next[1] - (current[1] + next[0]);
    }

    fn get_assertions(&self) -> Vec<Assertion<Self::BaseField>> {
        let last_step = self.trace_length() - 1;
        vec![
            Assertion::single(0, 0, Self::BaseField::ONE),
            Assertion::single(1, 0, Self::BaseField::ONE),
            Assertion::single(1, last_step, self.result),
        ]
    }
}

/// Parse public-input bytes. Wire format: single `BabyBear` field element
/// encoded as 4 bytes little-endian `u32`. Size is [`PUBLIC_SIZE`].
///
/// # Errors
///
/// Returns [`Error::PublicDeserialization`] if the buffer is shorter than
/// [`PUBLIC_SIZE`]. Any `u32` value is accepted, [`BaseElement::new`]
/// reduces non-canonical inputs mod `p`.
pub fn parse_public(bytes: &[u8]) -> Result<PublicInputs, Error> {
    // Strict length, same malleability rationale as the goldilocks sibling.
    if bytes.len() != PUBLIC_SIZE {
        return Err(Error::PublicDeserialization);
    }
    let chunk: &[u8; PUBLIC_SIZE] = bytes
        .first_chunk::<PUBLIC_SIZE>()
        .ok_or(Error::PublicDeserialization)?;
    let raw = u32::from_le_bytes(*chunk);
    Ok(PublicInputs {
        result: BaseElement::new(raw),
    })
}

/// Verify a Fibonacci-AIR STARK proof generated over `BabyBear` with the
/// quartic extension for the composition polynomial.
///
/// Hash: Blake3-256 over `BabyBear` (byte-oriented, so the hasher code is
/// identical to the Goldilocks path). Vector commitment: binary Merkle
/// tree. Random coin: winterfell's default. Security threshold: 95-bit
/// conjectured, matching Phase 3.2.
///
/// # Errors
///
/// Returns [`Error::Verification`] wrapping the inner [`VerifierError`] if
/// any Merkle / FRI / constraint check fails, *or* if the proof's
/// `ProofOptions` fall below the 95-bit conjectured-security threshold.
///
/// [`VerifierError`]: winterfell::VerifierError
pub fn verify(proof: Proof, public: PublicInputs) -> Result<(), Error> {
    type Hasher = Blake3_256<BaseElement>;
    type Coin = DefaultRandomCoin<Hasher>;
    type Vc = MerkleTree<Hasher>;

    // Field-modulus sanity: same check as the goldilocks sibling. Without
    // it, an 8-byte-field proof reaches `from_bytes_with_padding` inside
    // winterfell and trips the `bytes.len() < ELEMENT_BYTES` assert because
    // `BabyBear::ELEMENT_BYTES = 4`. Halt on embedded = DoS surface.
    if proof.context.field_modulus_bytes() != BaseElement::get_modulus_le_bytes().as_slice() {
        return Err(Error::ProofDeserialization);
    }

    let min_opts = AcceptableOptions::MinConjecturedSecurity(95);
    winterfell::verify::<FibAir, Hasher, Coin, Vc>(proof, public, &min_opts).map_err(Error::from)
}
