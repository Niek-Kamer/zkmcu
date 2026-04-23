#import "/research/lib/template.typ": *

// Phase 3.2.x diagnostic: isolate the variance-anomaly source flagged in
// phase 3.1 and reconfirmed in phase 3.2. Hypothesis under test: does
// proof.clone() inside the timed window dominate the iteration jitter?
// Answer: no. Clone contributes ~25 % of variance on M33 and is
// effectively neutral on RV32. Most jitter originates inside
// winterfell::verify itself.

#let m33_q       = toml("/benchmarks/runs/2026-04-24-m33-stark-fib-1024-q/result.toml")
#let rv32_q      = toml("/benchmarks/runs/2026-04-24-rv32-stark-fib-1024-q/result.toml")
#let m33_q_nc    = toml("/benchmarks/runs/2026-04-24-m33-stark-fib-1024-q-clone-hoisted/result.toml")
#let rv32_q_nc   = toml("/benchmarks/runs/2026-04-24-rv32-stark-fib-1024-q-clone-hoisted/result.toml")

#show: paper.with(
  title: "STARK verify variance: proof.clone() is not the dominant source",
  authors: ("zkmcu",),
  date: "2026-04-24",
  kind: "report",
  abstract: [
    Phase 3.1 flagged the STARK-verify variance anomaly (~0.3 % on M33,
    ~0.7 % on RV32, vs ~0.03--0.1 % for pairing-based verifiers on the
    same silicon). Phase 3.2 reconfirmed it. The working hypothesis
    across both reports was: `proof.clone()` sits inside the timed
    window because `winterfell::verify` consumes the proof; the
    allocator work contributes the jitter. This report tests that
    hypothesis directly by hoisting the clone outside the timed window
    and re-benching. *M33 variance drops from 0.33 % to 0.245 %
    (~25 % reduction). RV32 variance rises from 0.29 % to 0.46 %.*
    Neither lands near the silicon baseline. The clone is a minor
    contributor at best; the real jitter source is inside winterfell's
    verify path, most likely the internal `Vec` allocations for FRI
    state and auth-path parsing.
  ],
)

= The hypothesis under test

Phase 3.1 noted:

#quote([
  winterfell's verify takes `Proof` by value, consuming it. The clone
  allocates Vec buffers; the allocator path introduces jitter the
  pairing-based verifiers don't have because they don't allocate
  mid-verify.
])

The claim has been sitting in the notes for two benchmark runs. Easy to
test: hoist the clone outside the timed window, keep everything else
identical, measure.

= The experiment

Both firmware crates (`bench-rp2350-m33-stark`, `bench-rp2350-rv32-stark`)
were changed in a single commit. Diff per crate is effectively:

```rust
// Before (phase 3.1 / 3.2 firmware):
let t0 = cycle_count();
let result = verify(proof.clone(), public);  // clone inside window
let t1 = cycle_count();

// After (this experiment):
let cloned = proof.clone();                  // clone outside window
let t0 = cycle_count();
let result = verify(cloned, public);
let t1 = cycle_count();
```

No other firmware change. Same committed proof bytes. Same heap
arena. Same USB pacing.

= Results

#compare-table(
  ("Target", "Median (with clone)", "Median (no clone)", "Δ"),
  (
    ([Cortex-M33], [70.0 ms], [69.7 ms], [−0.3 ms]),
    ([Hazard3 RV32], [93.0 ms], [92.4 ms], [−0.6 ms]),
  ),
)

Clone cost *is* real and measurable: ~306 μs on M33, ~640 μs on RV32.
RV32 pays more for the clone — consistent with Hazard3's 64-bit
multiply story (allocator fast path isn't the bottleneck, but memcpy
of ~30 KB is).

#compare-table(
  ("Target", "Variance (with clone)", "Variance (no clone)", "Δ"),
  (
    ([Cortex-M33], [0.33 %], [*0.245 %*], [−25.8 %]),
    ([Hazard3 RV32], [0.29 %], [*0.456 %*], [*+57 %*]),
  ),
)

The two ISAs disagree. On M33, hoisting the clone *improves* variance
by ~25 %, consistent with "some of the jitter was the clone". On RV32,
hoisting the clone *worsens* variance by ~57 %, the opposite of
prediction.

Both post-hoist variances (0.245 % M33, 0.46 % RV32) are still an
order of magnitude above the ~0.03--0.1 % baseline that pairing-based
verifiers produce on this silicon. Silicon is not the limit.

= Why the ISAs disagree

Best interpretation: the clone's effect on variance depends on
*whether* the clone masks or exposes underlying jitter from *other*
allocations inside `winterfell::verify`.

/ On M33: the clone contributes its own jitter (~0.1 pp of variance),
  which adds to the in-verify allocator jitter. Removing the clone
  removes its contribution and exposes only the in-verify component.

/ On RV32: the clone takes ~640 μs (roughly 2× M33's 306 μs — memcpy
  bandwidth + allocator fast-path latency). Its fixed-cost presence
  appears to *stabilise* iteration timing — each iteration starts with
  a fresh, predictable allocator state after the clone completes.
  Without the clone, the allocator state carries over from the
  previous iteration's `Drop`, creating a more variable starting point.

Not a clean explanation, but both directions are consistent with the
bigger claim: *the real variance source is the in-verify allocations*,
and the clone sits on top of it as either a mild contributor (M33) or
a mild stabiliser (RV32) depending on allocator dynamics.

= What this rules in and rules out

*Rules out:* the simple "`proof.clone()` is the whole story" hypothesis
from phase 3.1. Hoisting the clone should have dropped variance by
~5--10× if the hypothesis were correct; measured drop was ~25 % on
M33 and negative on RV32.

*Rules in:* allocator jitter inside `winterfell::verify` as the
dominant variance source. The verify path allocates `Vec`s for:

- FRI layer state (per-layer, ~13 allocations per proof)
- Auth-path buffers (per-query × per-layer, ~32 × 13 = 416 allocations)
- Composition polynomial intermediate state
- Random coin seed buffers

All of these hit `embedded-alloc`'s `LlffHeap` path, which has
data-dependent fast/slow branching based on free-list state. Each
iteration's allocator state is slightly different from the previous
one, producing the observed 0.25--0.46 % iteration-to-iteration spread.

*Rules in (tentatively):* the variance is *not* related to cycle-count
noise, interrupts, or silicon non-determinism. All three would produce
symmetric variance independent of which operations are inside vs
outside the timed window.

= What to do about it

Three tiers of fixes, in order of engineering cost:

+ *Accept it and report both numbers.* 0.245 % variance is still
  comparable to typical production benchmark reporting. It's only
  notable relative to the 0.03 % baseline we know this silicon can
  produce on allocation-free workloads.

+ *Use a bump-arena allocator* (e.g. `bumpalo` or a fixed-size
  slab allocator) for the verify-internal `Vec`s. Would require
  either a fork of winterfell or convincing the upstream to make
  the allocator configurable. Non-trivial.

+ *Streaming verify* that pre-parses the proof into a bump arena
  once at startup and verifies without further allocation. Requires
  a custom no-alloc verify implementation. Major engineering,
  Phase 4+ work.

Recommendation: *(1)*. The variance level is acceptable; the
understanding is the deliverable. Phase 3 benchmarks keep reporting
both median and variance, and the variance anomaly gets explained in
documentation rather than engineered away.

= What this finding does not change

The phase 3.2 headline numbers stand:

- *M33 STARK verify*: 70.0 ms at 95-bit conjectured security, ~100 KB
  total RAM (31 KB margin under the 128 KB tier).
- *RV32 STARK verify*: 93.0 ms at 95-bit conjectured security.

The clone-hoisted numbers (69.7 ms / 92.4 ms) are the "verify only"
cost, useful as a reference for users who parse once and call verify
many times. The with-clone numbers are the "per-verify if you re-parse
or re-clone" cost, which is what naive usage produces.

= Limitations

- Single-proof, single-AIR (Fibonacci-1024 at Quadratic extension).
  Allocator patterns may look different for larger proofs or
  different AIRs.
- `embedded-alloc::LlffHeap` only. A different allocator (e.g.
  `linked-list-allocator` with a different free-list strategy, or
  a bump arena) would produce different variance numbers. Phase 3 has
  standardised on `LlffHeap` across all firmware crates for
  comparability; that standardisation is what's being measured here,
  not the allocator itself.
- ~22--25 sample sizes. Larger N might tighten the variance estimate;
  unlikely to change the qualitative finding.

#v(1em)

_Phase 3.2 results report_: `research/reports/2026-04-24-stark-quadratic-results.typ`.

_Phase 3.1 results report_: `research/reports/2026-04-23-stark-results.typ`.
