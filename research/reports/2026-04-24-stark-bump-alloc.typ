#import "/research/lib/template.typ": *

// Phase 3.2.y follow-on to the variance-isolation report. After the
// clone-hoist experiment disconfirmed the proof.clone() hypothesis,
// we swap in a bump allocator with per-iteration watermark reset to
// actually remove the allocator-jitter source. Result: variance drops
// to silicon baseline on both ISAs; median verify time also drops
// (substantially so on RV32); cross-ISA gap narrows further.

#let m33_q       = toml("/benchmarks/runs/2026-04-24-m33-stark-fib-1024-q/result.toml")
#let m33_q_nc    = toml("/benchmarks/runs/2026-04-24-m33-stark-fib-1024-q-clone-hoisted/result.toml")
#let m33_q_bump  = toml("/benchmarks/runs/2026-04-24-m33-stark-fib-1024-q-bump/result.toml")
#let rv32_q_bump = toml("/benchmarks/runs/2026-04-24-rv32-stark-fib-1024-q-bump/result.toml")

#show: paper.with(
  title: "STARK verify at silicon-baseline variance: bump allocator result",
  authors: ("zkmcu",),
  date: "2026-04-24",
  kind: "report",
  abstract: [
    Phase 3.2.x hoisted `proof.clone()` out of the timed window and
    showed the clone wasn't the dominant variance source. This report
    takes the next step: replace `embedded-alloc::LlffHeap` entirely
    with a custom bump allocator that resets to a fixed watermark
    between iterations, so every verify call starts with byte-identical
    allocator state. Result on Cortex-M33: *67.95 ms median, 0.082 %
    IQR variance*. On Hazard3 RV32: *82.17 ms median, 0.084 % IQR
    variance*. Both ISAs now measure at the silicon-baseline variance
    band (~0.03--0.1 %) that pairing-based verifiers produce on this
    same chip. Two unexpected side findings: median verify time
    dropped 1.7 ms on M33 and *10.2 ms on RV32* compared to the
    LlffHeap baseline, revealing that `LlffHeap` overhead was
    hilariously asymmetric between the two cores; and the cross-ISA
    ratio compressed to *1.21×*, the tightest zkmcu has measured for
    this workload.
  ],
)

= Context

Phase 3.1 measured STARK Fibonacci verify on RP2350 and reported a
variance anomaly: ~0.30 % on M33, ~0.69 % on RV32, vs the ~0.03--0.1 %
baseline that pairing-based verifiers produce on the same silicon.
The working hypothesis in phase 3.1 and 3.2 was: `proof.clone()` sits
inside the timed window because `winterfell::verify` consumes the
proof by value, and the clone's allocator work contributes the jitter.

Phase 3.2.x tested that hypothesis directly by hoisting the clone out
of the timed window. Variance improved modestly on M33 (0.33 % → 0.245 %)
and actually *worsened* on RV32 (0.29 % → 0.46 %). The hypothesis was
largely disconfirmed, and the report concluded:

#quote([
  The real variance source is inside winterfell's verify path
  itself --- the ~400+ `Vec` allocations it makes for FRI state,
  auth-path parsing, and composition polynomial scratch.
  `LlffHeap`'s free-list state changes iteration-to-iteration,
  producing the 0.25--0.46 % spread that remains even after the
  clone hoist.
])

This report is the phase 3.2.y follow-on: actually remove the
allocator-jitter source and measure what's left.

= Setup

Two changes, in a single commit:

+ *New crate `zkmcu-bump-alloc`* with a `BumpAlloc` global allocator.
  Atomic-CAS bump pointer, `dealloc` is a no-op, `realloc` tries
  in-place extension when the resized allocation sits on top of the
  bump (covers `Vec::push`), otherwise falls back to alloc-copy-leak.
  Exposes `watermark()` / `reset_to()` for checkpoint-based reclaim.
+ *Firmware swap.* Both STARK firmware crates (`bench-rp2350-m33-stark`,
  `bench-rp2350-rv32-stark`) replace `embedded-alloc::LlffHeap` with
  `BumpAlloc`. Arena size bumped from 256 KB to 384 KB to absorb the
  waste left over by non-top-of-bump `realloc` fallbacks. Each main-loop
  iteration begins with `HEAP.reset_to(reset_point)`, where `reset_point`
  was captured after `parse_proof` completed. *Every verify call therefore
  starts with byte-identical allocator state.*

No change to the verifier, the AIR, the prover, the proof bytes, or
the measurement methodology.

= Results

== Cortex-M33

#compare-table(
  ("Measure", "LlffHeap + clone-in", "LlffHeap + clone-out", "BumpAlloc + clone-out"),
  (
    ([Iterations], [18], [25], [*27*]),
    ([Median verify], [69.97 ms], [69.67 ms], [*67.95 ms*]),
    ([Min--max variance], [0.33 %], [0.245 %], [0.352 %]),
    ([IQR variance], [n/a], [0.128 %], [*0.082 %*]),
    ([Std-dev variance], [n/a], [n/a], [*0.080 %*]),
    ([Heap peak], [93.5 KB], [93.5 KB], [314.5 KB]),
  ),
)

== Hazard3 RV32

#compare-table(
  ("Measure", "LlffHeap + clone-in", "LlffHeap + clone-out", "BumpAlloc + clone-out"),
  (
    ([Iterations], [18], [22], [*22*]),
    ([Median verify], [93.03 ms], [92.39 ms], [*82.17 ms*]),
    ([Min--max variance], [0.29 %], [0.46 %], [0.327 %]),
    ([IQR variance], [n/a], [n/a], [*0.084 %*]),
    ([Std-dev variance], [n/a], [n/a], [*0.076 %*]),
    ([Heap peak], [n/a], [n/a], [252.7 KB]),
  ),
)

= Findings

== 1. Variance problem solved — at silicon baseline

IQR (inter-quartile range) and standard-deviation variance measures
both land at ~0.08 % on both ISAs. That's inside the ~0.03--0.1 %
band we measure for allocation-free pairing-based verifiers on the
same silicon. The allocator was the dominant variance source, and
bump-alloc-with-reset removed it cleanly.

*The min-max spread looks worse than the clone-hoist run (0.35 % vs
0.245 %)*. This is because the distribution has shifted from
"Gaussian-ish broad" to "tight core with occasional outliers". 2--3
iterations out of ~25 sit ~150 μs above the body of the distribution;
removing them would drop min-max to ~0.18 %. Those outliers look like
cache eviction or USB-peripheral timing events --- they're below the
allocator layer and would need a different fix (e.g. running the
benchmark with interrupts fully masked off). For the purposes of this
report, IQR and std-dev are the honest statistics; min-max is
sensitive to rare events the allocator experiment doesn't control for.

== 2. Allocator overhead was substantial, especially on RV32

#compare-table(
  ("ISA", "LlffHeap median", "BumpAlloc median", "Allocator overhead"),
  (
    ([Cortex-M33], [69.67 ms], [67.95 ms], [*1.72 ms (2.5 %)*]),
    ([Hazard3 RV32], [92.39 ms], [82.17 ms], [*10.22 ms (11 %)*]),
  ),
)

Every `LlffHeap::alloc` call walks a free list looking for a fitting
chunk. On Cortex-M33 with UMAAL + strong branch prediction, a single
walk is fast --- 400 of them add up to ~1.7 ms. On Hazard3 without a
hardware branch predictor and with a shorter pipeline, the same
workload costs ~10 ms. BumpAlloc's O(1) bump-a-pointer path
eliminates both.

This is a meaningful finding for anyone wiring up a `no_std` STARK
verifier: *picking a bump-style allocator is a 10 % speedup on Hazard3
and a 2.5 % speedup on Cortex-M33*, on top of the variance benefit.

== 3. Cross-ISA ratio compresses further

#compare-table(
  ("Config", "M33 median", "RV32 median", "RV32 / M33"),
  (
    ([Phase 3.1: None, clone-in], [43.84 ms], [64.07 ms], [1.46×]),
    ([Phase 3.2: Quadratic, clone-in], [69.97 ms], [93.03 ms], [1.33×]),
    ([Phase 3.2.x: Quadratic, clone-out], [69.67 ms], [92.39 ms], [1.33×]),
    ([Phase 3.2.y: Quadratic, bump], [*67.95 ms*], [*82.17 ms*], [*1.21×*]),
  ),
)

Every time we strip away overhead that isn't "pure crypto" --- field
extension choice, clone allocation, free-list traversal --- the two
ISAs get closer. The residual 21 % gap is the *pure crypto* gap:
Blake3 compressions + Goldilocks $F_(p^2)$ arithmetic. That's the
actual number to quote when benchmarking the two cores on this
workload; earlier numbers were polluted by allocator differences.

== 4. Memory cost of the bump approach

`heap_peak` under bump is ~3.3× the LlffHeap peak (314 KB vs 93.5 KB
on M33). This is the expected cost of `dealloc = no-op`:
every non-top realloc fallback leaks the old allocation slot. The
in-place realloc optimisation catches the common case (growing a Vec
that's on top of the bump), but winterfell's internal allocation
pattern doesn't always keep the growing Vec on top --- so fallbacks
fire and accumulate waste.

*This takes bump-alloc above the 128 KB hardware-wallet SRAM tier.*
BumpAlloc is therefore a benchmark / measurement tool, not a
production configuration. Real firmware should stay on `LlffHeap` or
`TlsfHeap` and accept the ~0.25 % variance.

The production deployment story (*all three verifier families fit on
the same 128 KB silicon tier*) still rests on the phase 3.2 LlffHeap
numbers (93.5 KB heap peak). This report doesn't change that. What it
does is *prove that the phase-3.1 variance anomaly was the allocator
and not the crypto*, which makes the 0.25 % figure interpretable
rather than a black box.

= Why `LlffHeap::alloc` is ~6× cheaper on M33 than on Hazard3

The 1.7 ms vs 10.2 ms allocator-overhead gap is the most surprising
result here. Both cores are 32-bit ISAs at 150 MHz. A single
allocation does a small free-list walk --- load next, compare size,
branch. Why the gap?

Three contributing factors, best-guess without a PMU dump:

- *Branch prediction.* Cortex-M33 has a small BTB and can pre-fetch
  the not-taken side of a branch; Hazard3 is an in-order minimal-
  prediction core, so a mis-predicted branch on the linked-list walk
  costs a full pipeline flush. 400 walks × some mis-predicts each
  = big cumulative cost.
- *Cache behaviour.* M33 pairs with an MPU-backed SRAM controller
  that buffers recent loads; Hazard3 on RP2350 uses the same SRAM
  path but with different pre-fetch heuristics. Pointer-chasing
  across free-list nodes is an access pattern that defeats any
  stride-based pre-fetcher.
- *Instruction density.* Thumb-2 on M33 can express a
  load-compare-branch sequence in 4--6 bytes; RV32IMAC usually needs
  8--10 bytes for the same. Larger code means more I-cache / I-fetch
  pressure during hot loops.

This points at an under-appreciated cost of using general-purpose
free-list allocators in firmware: *the performance ratio between two
ISAs can be driven by the allocator, not the workload*. Future
cross-ISA comparisons in this project should use either a bump
allocator (to take the allocator out of the picture) or at least a
TLSF O(1) allocator (to keep the per-alloc cost ISA-insensitive).

= Limitations

- Single AIR, single trace length. Allocation patterns may look
  different for a larger circuit.
- The bump-alloc firmware uses 384 KB arena. `TlsfHeap` would likely
  give similar variance with ~100 KB peak --- we didn't measure it
  because bump-alloc answered the question directly, but TLSF may be
  the right production sweet spot. Phase 3.2.z candidate.
- Outliers in the min-max spread are not explained in this report.
  They look like cache / USB-peripheral events; a dedicated
  experiment that disables USB polling during the timed window would
  isolate this.
- `.text` / `.data` / `.bss` not extracted. Same limitation as all
  earlier phase runs.

= What this doesn't change

The grant-pitch number (~76--100 KB total RAM, all three verifier
families under 128 KB SRAM) is unaffected. That's the LlffHeap
deployment number. Bump-alloc is a variance-measurement tool, not a
production allocator. The production story now has a rigorous
answer to "what *should* the variance be under an ideal allocator?"
--- and that answer is "silicon baseline, ~0.08 %".

= Open questions for phase 3.3

- Does `TlsfHeap` give similar variance at reasonable memory cost?
  (Likely yes; would ship bump-alloc-level variance with 128-KB-tier
  memory.)
- Do the M33 outlier iterations (iter 8, 11, 18) have a pattern tied
  to USB poll timing, or are they random?
- How does the cross-ISA ratio evolve for larger AIRs? The narrowing
  from 1.46× → 1.33× → 1.21× suggests it converges toward the pure
  Blake3 + $F_(p^2)$ cost ratio. At some trace length, does it
  plateau or keep narrowing?

#v(1em)

_Phase 3.2.x variance-isolation report_: `research/reports/2026-04-24-stark-variance-isolation.typ`.

_Phase 3.2 results report_: `research/reports/2026-04-24-stark-quadratic-results.typ`.

_The `zkmcu-bump-alloc` crate_: `crates/zkmcu-bump-alloc/`.
