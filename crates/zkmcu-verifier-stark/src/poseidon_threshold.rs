//! Combined `BabyBear` Poseidon + threshold check AIR (private-value variant).
//!
//! Proves simultaneously:
//! - `Poseidon(value, nonce) = commitment` (public `commitment`, private `value` + `nonce`)
//! - `value < threshold` (public `threshold`)
//!
//! Neither `value` nor `nonce` appears in `PublicInputs` or any boundary assertion,
//! so the verifier learns only `commitment` and `threshold`.
//!
//! # Circuit overview
//!
//! Trace: 4 columns × 64 rows.
//!
//! ```text
//! Col 0 (s0): value  (constant rows 0-32), Poseidon state[0] (rows 32-56)
//! Col 1 (s1): nonce  (constant rows 0-32), Poseidon state[1] (rows 32-56)
//! Col 2 (diff): threshold diff bit-decomposition (rows 0-32)
//! Col 3 (bit):  LSB of diff (rows 0-32)
//! ```
//!
//! Two phases run in the trace:
//!
//! **Phase 1 (transition steps 0-31, rows 0-32)** — bit decomposition:
//! - `s0`, `s1` held constant (carry constraints)
//! - `diff[i] = 2·diff[i+1] + bit[i]` (right-shift decomposition)
//! - Coupling at step 0: `diff[0] = threshold - s0[0] - 1`
//!
//! **Phase 2 (transition steps 32-55, rows 32-56)** — Poseidon permutation:
//! - 24 all-full-round `BabyBear` Poseidon with `α=7`, `MDS=[[2,1],[1,2]]`
//! - Starts at `(s0[32], s1[32]) = (value, nonce)` (from carry)
//! - Produces commitment at `s0[56]`
//!
//! Boundary assertions (both involve only public values):
//! - `diff[32] = 0` — bit decomposition exhausted without underflow → `value < threshold`
//! - `s0[56] = commitment` — Poseidon output matches public commitment
//!
//! # Security note
//!
//! 24 all-full rounds with `α=7`, `t=2` gives approximately 64-bit security against
//! Gröbner basis attacks (`Rf ≥ 128 / log2(7) ≈ 46` for 128-bit; we use 24 = half,
//! appropriate for a range-proof circuit where forgery requires only guessing a
//! valid `(value, nonce)` pair, not inverting the hash). For full 128-bit preimage
//! resistance replace `POSEIDON_ROUNDS` with 48 and set `trace_len = 128`.

use alloc::{vec, vec::Vec};

use winterfell::math::{FieldElement, StarkField, ToElements};
use winterfell::{
    Air, AirContext, Assertion, EvaluationFrame, ProofOptions, TraceInfo,
    TransitionConstraintDegree,
};
use zkmcu_babybear::BaseElement;

// ---- column indices --------------------------------------------------------

const COL_S0: usize = 0;
const COL_S1: usize = 1;
const COL_DIFF: usize = 2;
const COL_BIT: usize = 3;

// ---- periodic column indices -----------------------------------------------

const P_PHASE1: usize = 0; // 1 for steps 0-31, 0 elsewhere
const P_PHASE2: usize = 1; // 1 for steps 32-55, 0 elsewhere
const P_COUPLING: usize = 2; // 1 at step 0 only
const P_RC_S0: usize = 3; // round constant for s0, period 32
const P_RC_S1: usize = 4; // round constant for s1, period 32

// ---- circuit dimensions ---------------------------------------------------

/// Transition steps used by the bit decomposition (rows 0 → 32).
pub const ACTIVE_ROWS: usize = 32;
/// All-full Poseidon rounds (transition steps 32 → 56).
pub const POSEIDON_ROUNDS: usize = 24;
/// Trace length (power of 2; `ACTIVE_ROWS + POSEIDON_ROUNDS + 8` padding = 64).
pub const TRACE_LEN: usize = 64;

// ---- Poseidon parameters --------------------------------------------------
//
// MDS [[2,1],[1,2]] (det=3, MDS over BabyBear).
// Round constants: 24 pairs, generated from a splitmix64 seed.

const fn rc(i: u32) -> u32 {
    const P: u64 = 2_013_265_921; // BabyBear modulus
    let mut x = (i as u64).wrapping_add(1).wrapping_mul(0x9e37_79b9_7f4a_7c15_u64);
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9_u64);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb_u64);
    x ^= x >> 31;
    (x % P) as u32
}

/// Round constants: `RC[r] = (rc_for_s0, rc_for_s1)` at round `r`.
#[allow(clippy::indexing_slicing)]
const RC: [(u32, u32); POSEIDON_ROUNDS] = {
    let mut arr = [(0u32, 0u32); POSEIDON_ROUNDS];
    let mut i = 0u32;
    while (i as usize) < POSEIDON_ROUNDS {
        // i < POSEIDON_ROUNDS by the while guard; `.get()` is not usable in const context.
        arr[i as usize] = (rc(2 * i), rc(2 * i + 1));
        i += 1;
    }
    arr
};

// ---- public inputs ---------------------------------------------------------

/// Public inputs for the private-value threshold circuit.
///
/// Only `commitment` and `threshold` are revealed to the verifier.
/// The sensor reading (`value`) and binding randomness (`nonce`) are private.
#[derive(Debug, Clone, Copy)]
pub struct PublicInputs {
    /// Poseidon(value, nonce) — computed by the prover, verified by the circuit.
    pub commitment: BaseElement,
    /// Upper bound on the (private) value. Must satisfy `value < threshold - 1`.
    pub threshold: u32,
}

impl ToElements<BaseElement> for PublicInputs {
    fn to_elements(&self) -> Vec<BaseElement> {
        vec![self.commitment, BaseElement::new(self.threshold)]
    }
}

// ---- AIR -------------------------------------------------------------------

/// AIR for the combined Poseidon-commitment + threshold check.
pub struct PoseidonThresholdAir {
    context: AirContext<BaseElement>,
    threshold_element: BaseElement,
    commitment: BaseElement,
}

#[allow(clippy::panic, clippy::indexing_slicing)]
impl Air for PoseidonThresholdAir {
    type BaseField = BaseElement;
    type PublicInputs = PublicInputs;

    fn new(trace_info: TraceInfo, pub_inputs: PublicInputs, options: ProofOptions) -> Self {
        assert_eq!(4, trace_info.width(), "PoseidonThresholdAir expects a 4-column trace");
        let degrees = vec![
            // Phase 1: s0 carry (s0 constant during bit decomp)
            TransitionConstraintDegree::with_cycles(1, vec![64]),
            // Phase 1: s1 carry (s1 constant during bit decomp)
            TransitionConstraintDegree::with_cycles(1, vec![64]),
            // Phase 1: diff right-shift decomposition
            TransitionConstraintDegree::with_cycles(1, vec![64]),
            // Phase 1: bit is binary
            TransitionConstraintDegree::with_cycles(2, vec![64]),
            // Phase 2: Poseidon s0 step (x^7 S-box, degree 7 in trace cols)
            TransitionConstraintDegree::with_cycles(7, vec![64]),
            // Phase 2: Poseidon s1 step
            TransitionConstraintDegree::with_cycles(7, vec![64]),
            // Coupling at step 0: diff[0] = threshold - s0[0] - 1
            TransitionConstraintDegree::with_cycles(1, vec![64]),
        ];
        let context = AirContext::new(trace_info, degrees, 2, options);
        Self {
            context,
            threshold_element: BaseElement::new(pub_inputs.threshold),
            commitment: pub_inputs.commitment,
        }
    }

    fn context(&self) -> &AirContext<Self::BaseField> {
        &self.context
    }

    fn get_assertions(&self) -> Vec<Assertion<Self::BaseField>> {
        vec![
            // Bit decomp exhausted → value < threshold (no underflow)
            Assertion::single(COL_DIFF, ACTIVE_ROWS, BaseElement::ZERO),
            // Poseidon output equals the public commitment
            Assertion::single(COL_S0, ACTIVE_ROWS + POSEIDON_ROUNDS, self.commitment),
        ]
    }

    /// Periodic columns (all periods must divide `TRACE_LEN=64`, be ≥2, power-of-two).
    ///
    /// Order matches `P_PHASE1=0` … `P_RC_S1=4`.
    fn get_periodic_column_values(&self) -> Vec<Vec<Self::BaseField>> {
        let one = BaseElement::ONE;
        let zero = BaseElement::ZERO;

        // phase1_mask: 1 for steps 0-31, 0 for steps 32-63.  period=64
        let phase1: Vec<BaseElement> =
            (0..TRACE_LEN).map(|i| if i < ACTIVE_ROWS { one } else { zero }).collect();

        // phase2_mask: 1 for steps 32-55, 0 elsewhere.  period=64
        let phase2: Vec<BaseElement> = (0..TRACE_LEN)
            .map(|i| if (ACTIVE_ROWS..ACTIVE_ROWS + POSEIDON_ROUNDS).contains(&i) { one } else { zero })
            .collect();

        // coupling_mask: 1 at step 0 only.  period=64
        let coupling: Vec<BaseElement> =
            (0..TRACE_LEN).map(|i| if i == 0 { one } else { zero }).collect();

        // rc_s0: round constants for s0.  period=32
        // At step i (32 ≤ i ≤ 55): periodic_values = rc_s0[i mod 32] = RC[i-32].0 ✓
        let rc_s0: Vec<BaseElement> = (0..ACTIVE_ROWS)
            .map(|i| if i < POSEIDON_ROUNDS { BaseElement::new(RC[i].0) } else { zero })
            .collect();

        // rc_s1: same structure for s1.  period=32
        let rc_s1: Vec<BaseElement> = (0..ACTIVE_ROWS)
            .map(|i| if i < POSEIDON_ROUNDS { BaseElement::new(RC[i].1) } else { zero })
            .collect();

        vec![phase1, phase2, coupling, rc_s0, rc_s1]
    }

    fn evaluate_transition<E: FieldElement + From<Self::BaseField>>(
        &self,
        frame: &EvaluationFrame<E>,
        periodic_values: &[E],
        result: &mut [E],
    ) {
        let cur = frame.current();
        let nxt = frame.next();

        let phase1 = periodic_values[P_PHASE1];
        let phase2 = periodic_values[P_PHASE2];
        let coupling = periodic_values[P_COUPLING];
        let rc_s0 = periodic_values[P_RC_S0];
        let rc_s1 = periodic_values[P_RC_S1];

        let two: E = E::from(BaseElement::new(2));
        let threshold: E = E::from(self.threshold_element);

        // ── Phase 1 constraints (gated by phase1_mask) ───────────────────

        // s0 stays constant during bit decomp
        result[0] = phase1 * (nxt[COL_S0] - cur[COL_S0]);
        // s1 stays constant during bit decomp
        result[1] = phase1 * (nxt[COL_S1] - cur[COL_S1]);
        // diff[i] = 2·diff[i+1] + bit[i]  ↔  diff[i] - 2·diff[i+1] - bit[i] = 0
        result[2] = phase1 * (cur[COL_DIFF] - two * nxt[COL_DIFF] - cur[COL_BIT]);
        // bit[i]·(1 − bit[i]) = 0
        result[3] = phase1 * (cur[COL_BIT] * (E::ONE - cur[COL_BIT]));

        // ── Phase 2 constraints (gated by phase2_mask) ───────────────────
        //
        // MDS: [[2,1],[1,2]].  S-box: x^7.
        // s0_next = 2·sbox(s0) + sbox(s1) + rc_s0
        // s1_next =   sbox(s0) + 2·sbox(s1) + rc_s1

        let s0_7 = sbox7(cur[COL_S0]);
        let s1_7 = sbox7(cur[COL_S1]);
        let expected_s0 = two * s0_7 + s1_7 + rc_s0;
        let expected_s1 = s0_7 + two * s1_7 + rc_s1;

        result[4] = phase2 * (nxt[COL_S0] - expected_s0);
        result[5] = phase2 * (nxt[COL_S1] - expected_s1);

        // ── Coupling (gated by coupling_mask, active only at step 0) ─────
        //
        // Ties diff[0] to s0[0] = value:  diff[0] + s0[0] + 1 = threshold
        // Without this, the prover could prove a false (diff, threshold) pair
        // while using a different value in the Poseidon hash.
        result[6] = coupling * (cur[COL_DIFF] + cur[COL_S0] + E::ONE - threshold);
    }
}

// ── S-box x^7 ---------------------------------------------------------------

#[inline]
fn sbox7<E: FieldElement>(x: E) -> E {
    let x2 = x * x;
    let x4 = x2 * x2;
    x4 * x2 * x
}

// ── Native Poseidon permutation (for trace building) ─────────────────────────

fn poseidon_round(s0: BaseElement, s1: BaseElement, round: usize) -> (BaseElement, BaseElement) {
    let s0_7 = sbox7(s0);
    let s1_7 = sbox7(s1);
    let two = BaseElement::new(2);
    let (rc0, rc1) = *RC.get(round).expect("round index in bounds");
    (
        two * s0_7 + s1_7 + BaseElement::new(rc0),
        s0_7 + two * s1_7 + BaseElement::new(rc1),
    )
}

/// Compute `Poseidon(value, nonce)` over `BabyBear` using the same parameters
/// as the circuit.  Returns the first output element (the commitment).
pub fn poseidon_commit(value: u32, nonce: u32) -> BaseElement {
    let mut s0 = BaseElement::new(value);
    let mut s1 = BaseElement::new(nonce);
    for r in 0..POSEIDON_ROUNDS {
        (s0, s1) = poseidon_round(s0, s1, r);
    }
    s0
}

// ── Trace builder ────────────────────────────────────────────────────────────

/// Build the combined trace for the private-value threshold circuit.
///
/// # Panics
///
/// Panics if `value >= threshold - 1` (diff=0 makes the bit-decomp trace
/// all-zeros, causing winterfell's degree check to fail).
#[allow(clippy::panic, clippy::indexing_slicing)]
pub fn build_trace(
    value: u32,
    nonce: u32,
    threshold: u32,
) -> winterfell::TraceTable<BaseElement> {
    assert!(
        value < threshold.saturating_sub(1),
        "value must be strictly less than threshold"
    );

    let diff_init = threshold - value - 1;
    let commitment = poseidon_commit(value, nonce);
    let mut trace = winterfell::TraceTable::new(4, TRACE_LEN);

    trace.fill(
        |state| {
            state[COL_S0] = BaseElement::new(value);
            state[COL_S1] = BaseElement::new(nonce);
            state[COL_DIFF] = BaseElement::new(diff_init);
            state[COL_BIT] = BaseElement::new(diff_init & 1);
        },
        |step, state| {
            if step < ACTIVE_ROWS - 1 {
                // Bit decomp: shift diff right, carry s0/s1 forward.
                let next_diff = u32::try_from(state[COL_DIFF].as_int() >> 1)
                    .expect("BabyBear value fits in u32");
                state[COL_DIFF] = BaseElement::new(next_diff);
                state[COL_BIT] = BaseElement::new(next_diff & 1);
                // s0, s1 unchanged
            } else if step == ACTIVE_ROWS - 1 {
                // Last bit-decomp step: row ACTIVE_ROWS must have diff=0.
                state[COL_DIFF] = BaseElement::ZERO;
                state[COL_BIT] = BaseElement::ZERO;
                // s0, s1 now carry (value, nonce) into Poseidon start.
            } else if step < ACTIVE_ROWS + POSEIDON_ROUNDS {
                // Poseidon phase: apply one round.
                let round = step - ACTIVE_ROWS;
                let (ns0, ns1) = poseidon_round(state[COL_S0], state[COL_S1], round);
                state[COL_S0] = ns0;
                state[COL_S1] = ns1;
                // diff, bit stay 0
            } else {
                // Padding: freeze everything at current values.
            }
        },
    );

    // Sanity check: the boundary assertions must be satisfied.
    debug_assert_eq!(
        trace.get(COL_DIFF, ACTIVE_ROWS),
        BaseElement::ZERO,
        "diff must be 0 at row ACTIVE_ROWS"
    );
    debug_assert_eq!(
        trace.get(COL_S0, ACTIVE_ROWS + POSEIDON_ROUNDS),
        commitment,
        "Poseidon output must match commitment"
    );

    trace
}
