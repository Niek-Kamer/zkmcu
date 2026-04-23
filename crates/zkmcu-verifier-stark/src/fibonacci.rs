//! Fibonacci AIR — the STARK hello-world.
//!
//! The circuit has a 2-column trace `(s_0, s_1)`:
//!
//! - `s_{0, i+1} = s_{0, i} + s_{1, i}`
//! - `s_{1, i+1} = s_{1, i} + s_{0, i+1}`
//!
//! Assertions tie the public inputs:
//!
//! - `s_{0, 0} = 1`  (initial value)
//! - `s_{1, 0} = 1`  (initial value)
//! - `s_{1, last} = result` (the claimed N-th Fibonacci number)
//!
//! At step `i`, column `s_0` holds `Fib(2i + 1)` and column `s_1` holds
//! `Fib(2i + 2)`. For a trace of length `N = 1024`, the asserted result
//! is `Fib(2048)` over the Goldilocks field.
//!
//! This matches the AIR that `zkmcu-host-gen` uses on the prover side —
//! the two sides MUST agree exactly on trace width, constraint degrees,
//! number of assertions, and their positions. Any drift and the proof
//! will decode but fail verification with a constraint-violation error.

use alloc::{vec, vec::Vec};

use winterfell::crypto::hashers::Blake3_256;
use winterfell::crypto::{DefaultRandomCoin, MerkleTree};
use winterfell::math::fields::f64::BaseElement;
use winterfell::math::{FieldElement, ToElements};
use winterfell::{
    AcceptableOptions, Air, AirContext, Assertion, EvaluationFrame, Proof, ProofOptions, TraceInfo,
    TransitionConstraintDegree,
};

use crate::Error;

/// Serialised size of the public-input wire format: one Goldilocks field
/// element as an 8-byte little-endian `u64`.
pub const PUBLIC_SIZE: usize = 8;

/// Public inputs for the Fibonacci AIR: the claimed N-th Fibonacci value.
#[derive(Debug, Clone, Copy)]
pub struct PublicInputs {
    /// The claimed result — `Fib(2 * trace_length)` over Goldilocks.
    pub result: BaseElement,
}

impl ToElements<BaseElement> for PublicInputs {
    fn to_elements(&self) -> Vec<BaseElement> {
        vec![self.result]
    }
}

/// Algebraic-intermediate-representation for the Fibonacci sequence.
pub struct FibAir {
    context: AirContext<BaseElement>,
    result: BaseElement,
}

// The `Air::new` contract requires trace-width / degree / assertion-count
// invariants to hold at construction. They're enforced via `assert_eq!`,
// which clippy flags as a panic path — but this is required by the
// upstream trait and called by the winterfell verifier internally, not
// by external user input.
//
// `evaluate_transition` indexes `current[0..1]` / `next[0..1]` / `result[0..1]`
// by position — the Air trait's contract guarantees these slices are sized
// to match the declared trace width (2) and constraint count (2) that we
// fixed in `new()`. Silencing `indexing_slicing` here is the cleanest
// expression of the constraint math.
#[allow(clippy::panic, clippy::indexing_slicing)]
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

/// Parse public-input bytes. Wire format: single Goldilocks field element
/// encoded as 8 bytes little-endian `u64`. Size is [`PUBLIC_SIZE`].
///
/// # Errors
///
/// Returns [`Error::PublicDeserialization`] if the buffer is shorter than
/// [`PUBLIC_SIZE`]. The field element reduction is handled by
/// [`BaseElement::new`] — any `u64` value in `[0, 2^64)` is accepted.
pub fn parse_public(bytes: &[u8]) -> Result<PublicInputs, Error> {
    let chunk: &[u8; PUBLIC_SIZE] = bytes
        .first_chunk::<PUBLIC_SIZE>()
        .ok_or(Error::PublicDeserialization)?;
    let raw = u64::from_le_bytes(*chunk);
    Ok(PublicInputs {
        result: BaseElement::new(raw),
    })
}

/// Verify a Fibonacci-AIR STARK proof against the provided public inputs.
///
/// Hash: Blake3-256 over Goldilocks. Vector commitment: binary Merkle tree.
/// Random coin: winterfell's default. Security threshold: 63-bit
/// conjectured (matches the base Goldilocks field with no extension).
///
/// 63-bit conjectured security is low for production use. The winterfell
/// docstring example hits 95-bit by using the `f128` field;
/// alternatively, setting `FieldExtension::Quadratic` in the prover's
/// `ProofOptions` lifts Goldilocks to ~128-bit. For phase 3.1 benchmarking
/// the per-query cost is the same regardless of claimed security, so we
/// accept what the base-field options provide — security hardening is a
/// phase-3.2 follow-up.
///
/// # Errors
///
/// Returns [`Error::Verification`] wrapping the inner [`VerifierError`] if
/// any of the Merkle / FRI / constraint checks fail.
///
/// [`VerifierError`]: winterfell::VerifierError
pub fn verify(proof: Proof, public: PublicInputs) -> Result<(), Error> {
    type Hasher = Blake3_256<BaseElement>;
    type Coin = DefaultRandomCoin<Hasher>;
    type Vc = MerkleTree<Hasher>;

    let min_opts = AcceptableOptions::MinConjecturedSecurity(63);
    winterfell::verify::<FibAir, Hasher, Coin, Vc>(proof, public, &min_opts).map_err(Error::from)
}
