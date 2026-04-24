#import "/research/lib/template.typ": *

#show: paper.with(
  title: "bench-core refactor unlocked 23 % M33 speedup on BabyBear × Quartic",
  authors: ("zkmcu",),
  date: "2026-04-24",
  kind: "postmortem",
  abstract: [
    Extracted shared firmware helpers (USB, clocks, stack painting, cycle counter, peak-tracking heap) out of the six `bench-rp2350-*` crates into a new `crates/bench-core/` library and rewrote the M33 binaries on top of it. No verifier / no_std-crate / winterfell code was touched. Unexpected result: M33 STARK verify with BabyBear × Quartic-Karatsuba dropped 23 % (124.22 → 95.63 ms). No other bench moved more than ~2 % either way. Working hypothesis: closure-wrapping the verify call changes register allocation in `ExtensibleField<4>::mul`, and that hot loop was at the Thumb-2 spill boundary.
  ],
)

= What changed, and what didn't

Only the firmware harness. `find crates vendor -name '*.rs' -newer <baseline TOML>` returns only the new `bench-core/src/*`.

The measurement idiom changed from this:

```rust
let cloned = proof.clone();
let t0 = DWT::cycle_count();
let result = fib::verify(cloned, public);
let t1 = DWT::cycle_count();
let cycles = u64::from(t1.wrapping_sub(t0));
```

to this:

```rust
let cloned = proof.clone();
let (result, cycles) = measure_cycles(|| fib::verify(cloned, public));
```

`measure_cycles` is `#[inline]` and compiles to the same two DWT reads + `wrapping_sub`. Net change at the source level: wrapping the verify call in a `FnOnce` closure.

= Measured deltas

M33 at 150 MHz, TLSF heap, Fibonacci-1024 AIR. `heap_peak` byte-identical across ISAs on every binary (81 888 B BN-square, 79 360 B BLS12-square, 93 515 B STARK-Goldilocks, 91 362 B STARK-BabyBear). `stack` grew ~150-250 B from the closure frame. All verdicts `ok=true`.

#table(
  columns: (auto, auto, auto, auto),
  align: (left, right, right, right),
  stroke: 0.4pt + luma(200),
  [*Proof system*], [*Baseline*], [*bench-core*], [*Delta*],
  [M33 Groth16 BN254],              [641 ms],     [642 ms],     [+0.2 %],
  [M33 Groth16 BLS12-381],          [2 014 ms],   [1 997 ms],   [−0.8 %],
  [M33 STARK Goldilocks-Quadratic], [74.6 ms],    [73.2 ms],    [−1.9 %],
  [*M33 STARK BabyBear-Quartic-K*], [*124.22 ms*], [*95.63 ms*], [*−23.02 %*],
  [RV32 Groth16 BN254],             [1 327 ms],   [1 362 ms],   [+2.7 %],
  [RV32 Groth16 BLS12-381],         [5 150 ms],   [5 195 ms],   [+0.9 %],
  [RV32 STARK Goldilocks-Quadratic],[112.4 ms],   [107.96 ms],  [−3.95 %],
  [RV32 STARK BabyBear-Quartic-K],  [129.05 ms],  [128.02 ms],  [−0.80 %],
)

The RV32 Groth16 regressions (+2.7 %, +0.9 %) are explained, not mysterious: RV32 baselines ran plain `LlffHeap` while bench-core upgrades RV32 to `TrackingLlff`. Two relaxed atomics per alloc, scaled by `substrate-bn` / `bls12_381`'s inner-loop allocation count. Observability gain, not perf loss.

= Working hypothesis

BabyBear's `ExtensibleField<4>::mul` touches 4 base-field limbs per extension element; the quartic mul/square hot loop is register-tight on Cortex-M33 (13 general-purpose registers usable in Thumb-2 after reserving LR/SP/PC and a couple of callee-saved).

Wrapping the verify call in a `FnOnce` closure changes where the captured `cloned` and `public` live in the stack frame and wich registers survive across the closure call boundary. The register allocator apparently flips at least one spill in the hot extension-mul inner loop to a register-resident value. Goldilocks's quadratic extension is 2-element, has register slack, unaffected.

Cross-ISA evidence supports this: M33 moved 124.22 → 95.63 ms (−23 %) while RV32 moved 129.05 → 128.02 ms (−0.8 %) on the exact same source change. 30× magnitude gap in the delta between ISAs. Ofcourse no plausible uniform-codegen shift (icache, inlining, function ordering) would produce a 30× asymmetry.

RV32IMAC has 31 GPRs. RV32 was never register-tight, so there was nothing for the closure-wrap to unlock. Thumb-2 with ~13 usable GPRs was at the spill boundary and picked up the whole win.

Still a hypothesis, not a confirmation. Confirming would need `cargo asm` or ELF disassembly of `QuartExtension::mul` old-vs-new. Deferred, the speedup is a win either way and the phase 3.3 story changes with the number, not the mechanism.

= What this invalidates

- Any earlier Typst report citing 124 ms M33 BabyBear needs a rerun before publication. The revised ratio is +30 % slower than Goldilocks × Quadratic (95.63 / 73.2), not +66 %. The headline conclusion stands: BabyBear × Quartic does not beat Goldilocks × Quadratic. The magnitude does not stand.

= What to re-measure

All four TLSF baselines (M33 + RV32 × Goldilocks + BabyBear) under the bench-core firmware, before writing any phase 3.3 report. The earlier 2026-04-24 TOMLs were taken with the pre-refactor firmware and should stay untouched as the pre-refactor record. New runs go in new dated directories.

= Rule

A firmware harness is not neutral. Extracting shared code into a library can flip register allocation on a register-tight hot loop in a direction that dwarfs the thing you're measuring. Keep the harness frozen across any single cross-configuration comparison, and re-measure everything whenever it changes. This one bit us in the right direction; next time it might not.
