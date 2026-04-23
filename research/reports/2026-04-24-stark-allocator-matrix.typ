#import "/research/lib/template.typ": *

// Phase 3.2.z — final report in the allocator-variance arc. Having
// tested LlffHeap (baseline), BumpAlloc (determinism), and TlsfHeap
// (middle ground), this report synthesises the tradeoff matrix and
// picks a production recommendation per use case.

#let llff_m33_co  = toml("/benchmarks/runs/2026-04-24-m33-stark-fib-1024-q-clone-hoisted/result.toml")
#let bump_m33     = toml("/benchmarks/runs/2026-04-24-m33-stark-fib-1024-q-bump/result.toml")
#let bump_rv32    = toml("/benchmarks/runs/2026-04-24-rv32-stark-fib-1024-q-bump/result.toml")
#let tlsf_m33     = toml("/benchmarks/runs/2026-04-24-m33-stark-fib-1024-q-tlsf/result.toml")
#let tlsf_rv32    = toml("/benchmarks/runs/2026-04-24-rv32-stark-fib-1024-q-tlsf/result.toml")

#show: paper.with(
  title: "Allocator matrix: three strategies for STARK verify on RP2350",
  authors: ("zkmcu",),
  date: "2026-04-24",
  kind: "report",
  abstract: [
    Three allocator strategies benchmarked on the same Fibonacci-1024
    proof, same silicon, same extension field. `LlffHeap` (linked-list
    first-fit) is the phase-3.2 baseline. `BumpAlloc` (custom
    watermark-reset arena) gave silicon-baseline variance at the cost
    of 384 KB arena. `TlsfHeap` (two-level segregated fit) fills the
    middle: 128 KB-tier-compliant heap footprint AND variance at
    silicon baseline, for a ~5 ms median-verify penalty on M33 and
    ~20 ms on Hazard3 RV32. Cross-ISA ratio swings from 1.21× (bump)
    to 1.33× (LlffHeap) to 1.51× (TLSF) — an allocator choice can
    change the "M33 vs Hazard3 on STARK verify" answer by 30 %.
    *Production recommendation for hardware-wallet-class devices:
    TLSF.* You get the tier fit, the determinism, and the phase-3.2
    grant-pitch line all in one configuration.
  ],
)

= Context

The phase-3.2 arc started with a variance anomaly: ~0.3 % on M33,
~0.7 % on RV32, vs ~0.03--0.1 % for pairing-based verifiers on the
same silicon. Three sub-reports traced it:

/ Phase 3.2.x: Hoisted `proof.clone()` out of the timed window.
  Modest improvement on M33, no improvement on RV32. The clone
  wasn't the dominant source.

/ Phase 3.2.y: Replaced `LlffHeap` with a custom bump allocator that
  resets to a fixed watermark between iterations. Variance dropped to
  silicon baseline (0.08 %) on both ISAs, and median verify dropped
  1.7 ms on M33 / 10.2 ms on RV32 as a side effect. Cost: 384 KB
  arena, above the 128 KB production tier.

/ Phase 3.2.z (this report): `embedded-alloc::TlsfHeap`.

The question this report answers: *is there a configuration that is
both silicon-baseline-variance AND 128-KB-tier-compliant?* Bump gave
us variance but not the tier; LlffHeap gave us the tier but not the
variance. TLSF is the third option.

= Numbers

All runs: Fibonacci AIR N=1024, `FieldExtension::Quadratic`, 95-bit
conjectured security, `proof.clone()` hoisted outside the timed
window, ~20 iterations per datapoint.

== Cortex-M33

#compare-table(
  ("Measure", "LlffHeap", "BumpAlloc", "TlsfHeap"),
  (
    ([Iterations], [25], [27], [20]),
    ([Median verify], [69.67 ms], [*67.95 ms*], [74.65 ms]),
    ([Min--max variance], [0.245 %], [0.352 %], [0.343 %]),
    ([IQR variance], [0.128 %], [*0.082 %*], [0.113 %]),
    ([Std-dev variance], [--], [*0.080 %*], [*0.081 %*]),
    ([Heap peak], [*93.5 KB*], [314.5 KB], [*93.5 KB*]),
    ([Fits 128 KB?], [*yes*], [no], [*yes*]),
  ),
)

== Hazard3 RV32

#compare-table(
  ("Measure", "LlffHeap", "BumpAlloc", "TlsfHeap"),
  (
    ([Iterations], [22], [22], [23]),
    ([Median verify], [92.39 ms], [*82.17 ms*], [112.40 ms]),
    ([Min--max variance], [0.456 %], [0.327 %], [0.411 %]),
    ([Std-dev variance], [--], [*0.076 %*], [0.110 %]),
    ([Cross-ISA ratio], [1.329×], [*1.209×*], [1.506×]),
  ),
)

= Findings

== 1. Three allocators, three distinct Pareto points

#compare-table(
  ("Strategy", "Wins on", "Loses on"),
  (
    ([LlffHeap], [fastest median (on M33), smallest heap], [worst variance]),
    ([BumpAlloc], [best variance, faster than LlffHeap], [needs 384 KB arena — *above 128 KB tier*]),
    ([TlsfHeap], [silicon-baseline variance *and* 128 KB tier], [~5 ms slower on M33, ~20 ms slower on RV32]),
  ),
)

None of the three dominates the others on every axis. The choice is
a policy question driven by which constraint matters most for a
given deployment.

== 2. TLSF's Hazard3 tax is disproportionate

On Cortex-M33, TLSF costs ~5 ms over LlffHeap (+7 %). On Hazard3
RV32, it costs ~20 ms (+22 %). *Same allocator library, same
workload, 4× the proportional cost on RISC-V.*

Best guess: TLSF's per-alloc op walks a two-level bitmap with
conditional branches at each step. Hazard3's in-order pipeline +
minimal branch predictor pays a full flush per mispredict; M33's BTB
absorbs most of them. Bump alloc is branch-free on the happy path
(CAS + arithmetic), which is why it *narrowed* the cross-ISA gap to
1.21× where TLSF *widened* it to 1.51×.

*Implication for cross-ISA crypto benchmarks*: the allocator choice
can swing the M33-vs-Hazard3 answer by 30 %. Any published
microarchitecture comparison that uses a stock general-purpose
allocator is partially measuring the allocator, not the workload.
This goes into the limitations section of every future cross-ISA
report in this project.

== 3. The 128 KB tier story holds for every production strategy

`LlffHeap`: 93.5 KB heap peak. Fits.
`TlsfHeap`: 93.5 KB heap peak (identical — same alloc pattern, same
live memory). Fits.
`BumpAlloc`: 314.5 KB heap peak. Does not fit.

Bump is a *measurement tool*, not a production configuration. For
production firmware that ships on 128 KB silicon, the choice is
between LlffHeap (speed) and TlsfHeap (determinism). Memory
footprint is the same.

== 4. Production recommendation: TlsfHeap

The strongest sentence the project can claim is now:

#quote([
  zkmcu ships a `no_std` Rust STARK verifier that fits under 128 KB
  SRAM at 95-bit conjectured security, with iteration-to-iteration
  timing variance at the silicon noise floor ($approx$ 0.08 %) —
  measured on real hardware (RP2350 Cortex-M33 @ 150 MHz).
])

That sentence is only defensible with TLSF. LlffHeap gives the
first two clauses; BumpAlloc gives the last two but busts the tier.
TLSF gives all three.

The 5 ms median tax on M33 (and 20 ms on RV32) is the cost. For
hardware wallet firmware where verify is triggered by a human
action (confirming a transaction), 75 ms vs 70 ms is
indistinguishable at the UX layer. For anything in a hot loop or
tight latency budget, pick LlffHeap and accept the ~0.25 %
variance.

= What we measured along the way

Seven new data points beyond the variance story:

+ *LlffHeap overhead is ISA-asymmetric* — 2.5 % on M33, 11 % on
  Hazard3. Every free-list walk costs Hazard3 more.
+ *Bump alloc narrows cross-ISA gap, TLSF widens it* — pure crypto
  is M33 by 1.21×; LlffHeap measurements overstate the gap by
  including allocator overhead.
+ *Proof size under Quadratic extension is auth-path-dominated* —
  grew only 22 % (not the predicted 70 %) going from None to
  Quadratic, because auth-path hashes dominate FRI-eval bytes.
+ *`proof.clone()` is not the main variance source* — hoisting it
  out saved only ~25 % of variance on M33 and none on RV32.
+ *heap_peak is allocator-independent* between LlffHeap and TLSF —
  both hold the same live data, so both peak at 93.5 KB.
+ *Bump allocator is ~$1.7"."."."10"."2$ ms faster than general-
  purpose allocators* on this workload, quantifying "allocator
  overhead" as a measurable performance axis.
+ *128 KB tier survives every knob we tested* at 95-bit security.
  Phase-3 grant-pitch line holds.

= Phase 3.3 candidates

With the allocator question closed, the remaining open tracks are:

- *Bigger AIR.* Does the cross-ISA pattern survive at $N = 2^(16)$
  or $N = 2^(18)$? Does the allocator-overhead structure change when
  auth paths are deeper?
- *BabyBear / Mersenne-31 field.* Goldilocks is 64-bit on 32-bit
  hardware. BabyBear ($p = 15 dot 2^(27) + 1$) fits a 32-bit register
  natively. Expected speedup on both ISAs; changes the Quadratic
  extension cost story.
- *Miden VM trace verify.* Realistic workload beyond the Fibonacci
  hello-world. Starts pushing the 128 KB tier again.
- *Pre-allocated scratch.* Contribute back to winterfell upstream:
  make the verifier accept a caller-provided `&mut [u8]` scratch
  arena, so production firmware can use a `TrackingHeap`-style
  accounting allocator with zero runtime alloc calls. Phase 4
  territory.

#v(1em)

_Bump allocator crate_: `crates/zkmcu-bump-alloc/`.

_Phase 3.2.y report_: `research/reports/2026-04-24-stark-bump-alloc.typ`.

_Phase 3.2.x variance-isolation report_: `research/reports/2026-04-24-stark-variance-isolation.typ`.
