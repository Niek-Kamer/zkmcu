//! Threshold check AIR: prove `value < threshold` for public `value` and `threshold`.
//!
//! Method: bit-decompose `diff = threshold - value - 1`. If this is a valid
//! non-negative value in the `BabyBear` field, then `threshold - value - 1 ≥ 0`,
//! i.e., `value < threshold`.
//!
//! Trace layout: 2 columns × 64 rows.
//!
//! ```text
//! col 0 (remaining): diff >> i  at row i  (shifts right each step)
//! col 1 (bit):       (diff >> i) & 1       (LSB of remaining)
//! ```
//!
//! Rows 0–31: active bit decomposition of `diff`.
//! Row 32:    must equal 0 (boundary assertion — proves diff < 2^32, no underflow).
//! Rows 33–63: all zeros (padding, forced by transition constraints).
//!
//! Transition constraint (degree 1) for all rows 0–62:
//! ```text
//!   remaining[i]  =  2 · remaining[i+1]  +  bit[i]
//! ```
//!
//! Bit constraint (degree 2) for all rows 0–62:
//! ```text
//!   bit[i] · (1 − bit[i])  =  0
//! ```
//!
//! Boundary assertions:
//! ```text
//!   remaining[0]  =  threshold − value − 1   (pins the specific claim)
//!   remaining[32] =  0                        (no underflow → value < threshold)
//! ```
//!
//! # Use case
//!
//! An embedded device proves that its sensor reading `value` is below a
//! public safety threshold `threshold`. Both are public — this is
//! *verifiable computation*, not privacy. The STARK provides an unforgeable
//! attestation: the device cannot report a valid proof while lying about
//! its reading.
//!
//! Values must satisfy `value < threshold - 1 < BabyBear modulus` (~2^31).
//! When `diff = 0` (i.e. `value = threshold - 1`) the trace is all-zeros and
//! winterfell's constraint-degree check panics — the tightest provable claim
//! is `value = threshold - 2`.
//!
//! Privacy variant (value private, commitment public) requires a hash
//! function inside the circuit and is left for a future Poseidon AIR.

use alloc::{vec, vec::Vec};

use winterfell::math::{FieldElement, StarkField, ToElements};
use winterfell::{
    Air, AirContext, Assertion, EvaluationFrame, ProofOptions, TraceInfo,
    TransitionConstraintDegree,
};
use zkmcu_babybear::BaseElement;

/// Column index for the running remaining value.
const COL_REMAINING: usize = 0;
/// Column index for the current bit (LSB of remaining).
const COL_BIT: usize = 1;

/// Number of rows used for the actual 32-bit bit decomposition.
pub const ACTIVE_ROWS: usize = 32;
/// Total trace length (power of 2, ≥ `ACTIVE_ROWS` + 1 for the zero boundary).
pub const TRACE_LEN: usize = 64;

/// Public inputs: both the value being checked and the threshold are revealed.
///
/// The proof attests that `value < threshold`. Both must be less than the
/// `BabyBear` modulus (2^31 − 2^27 + 1 ≈ 2 billion). A future Poseidon AIR can
/// make `value` private by replacing it with a hash commitment.
#[derive(Debug, Clone, Copy)]
pub struct PublicInputs {
    /// The sensor reading being proved below threshold.
    pub value: u32,
    /// The upper bound. Must satisfy `value < threshold < BabyBear modulus`.
    pub threshold: u32,
}

impl ToElements<BaseElement> for PublicInputs {
    fn to_elements(&self) -> Vec<BaseElement> {
        vec![
            BaseElement::new(self.value),
            BaseElement::new(self.threshold),
        ]
    }
}

/// AIR for the threshold check circuit.
pub struct ThresholdAir {
    context: AirContext<BaseElement>,
    /// `threshold - value - 1`, pinned at row 0 via boundary assertion.
    diff: BaseElement,
}

// Silencing `indexing_slicing`: the frame slices are sized by winterfell to
// match the declared trace width (2) and constraint count (2). Same rationale
// as `FibAir` — the trait contract guarantees the sizes.
#[allow(clippy::panic, clippy::indexing_slicing)]
impl Air for ThresholdAir {
    type BaseField = BaseElement;
    type PublicInputs = PublicInputs;

    fn new(trace_info: TraceInfo, pub_inputs: PublicInputs, options: ProofOptions) -> Self {
        assert_eq!(
            2,
            trace_info.width(),
            "ThresholdAir expects a 2-column trace"
        );
        let degrees = vec![
            TransitionConstraintDegree::new(1), // shift constraint (degree 1)
            TransitionConstraintDegree::new(2), // bit constraint   (degree 2)
        ];
        let context = AirContext::new(trace_info, degrees, 2, options);
        // checked_sub runs on the verifier side too — rejects false claims
        // before any FRI work happens. Plain u32 subtraction would wrap in
        // release mode and produce a field element that satisfies remaining[32]=0
        // trivially (all BabyBear elements are < 2^32), so the wrap must be
        // caught here rather than relying on the constraint system.
        let diff = pub_inputs
            .threshold
            .checked_sub(pub_inputs.value)
            .and_then(|v| v.checked_sub(1))
            .expect("ThresholdAir: value must be strictly less than threshold");
        Self {
            context,
            diff: BaseElement::new(diff),
        }
    }

    fn context(&self) -> &AirContext<Self::BaseField> {
        &self.context
    }

    fn get_assertions(&self) -> Vec<Assertion<Self::BaseField>> {
        vec![
            // remaining[0] = diff (binds the proof to specific value and threshold)
            Assertion::single(COL_REMAINING, 0, self.diff),
            // remaining[32] = 0 (all bits consumed without underflow → diff < 2^32)
            Assertion::single(COL_REMAINING, ACTIVE_ROWS, BaseElement::ZERO),
        ]
    }

    fn evaluate_transition<E: FieldElement + From<Self::BaseField>>(
        &self,
        frame: &EvaluationFrame<E>,
        _periodic_values: &[E],
        result: &mut [E],
    ) {
        let cur = frame.current();
        let nxt = frame.next();
        // remaining[i] = 2·remaining[i+1] + bit[i]
        // rewritten: remaining[i] - (remaining[i+1] + remaining[i+1]) - bit[i] = 0
        result[COL_REMAINING] =
            cur[COL_REMAINING] - (nxt[COL_REMAINING] + nxt[COL_REMAINING]) - cur[COL_BIT];
        // bit[i]·(1 - bit[i]) = 0
        result[COL_BIT] = cur[COL_BIT] * (E::ONE - cur[COL_BIT]);
    }
}

/// Build the 2-column × 64-row trace for a threshold check.
///
/// # Panics
///
/// Panics if `value >= threshold` (the claimed statement would be false).
#[allow(clippy::panic, clippy::indexing_slicing)]
pub fn build_trace(value: u32, threshold: u32) -> winterfell::TraceTable<BaseElement> {
    // diff=0 (value = threshold-1) makes the trace all-zeros, causing winterfell's
    // constraint degree check to panic. Require value < threshold - 1.
    assert!(
        value < threshold.saturating_sub(1),
        "value must be strictly less than threshold"
    );
    let diff = threshold - value - 1;
    let mut trace = winterfell::TraceTable::new(2, TRACE_LEN);
    trace.fill(
        |state| {
            state[COL_REMAINING] = BaseElement::new(diff);
            state[COL_BIT] = BaseElement::new(diff & 1);
        },
        |step, state| {
            if step < ACTIVE_ROWS - 1 {
                // BabyBear values are < 2^31, so u64 → u32 is always safe here.
                let next_val = u32::try_from(state[COL_REMAINING].as_int() >> 1)
                    .expect("BabyBear value fits in u32");
                state[COL_REMAINING] = BaseElement::new(next_val);
                state[COL_BIT] = BaseElement::new(next_val & 1);
            } else {
                state[COL_REMAINING] = BaseElement::ZERO;
                state[COL_BIT] = BaseElement::ZERO;
            }
        },
    );
    trace
}
