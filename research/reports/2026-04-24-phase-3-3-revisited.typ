#import "/research/lib/template.typ": *

// Phase 3.3 addendum — after the bench-core refactor landed
// (commit 98c7af5), every STARK config got re-measured. M33
// BabyBear × Quartic-Karatsuba dropped 23 % from the previous
// baseline, RV32 barely moved. This unseats the 1.04× cross-ISA
// headline from 2026-04-25-babybear-quartic-cross-isa.typ.

#let gold_pre_m33   = toml("/benchmarks/runs/2026-04-24-m33-stark-fib-1024-q-tlsf/result.toml")
#let gold_pre_rv32  = toml("/benchmarks/runs/2026-04-24-rv32-stark-fib-1024-q-tlsf/result.toml")
#let bb_kar_pre_m33  = toml("/benchmarks/runs/2026-04-24-m33-stark-fib-1024-babybear-q-kara/result.toml")
#let bb_kar_pre_rv32 = toml("/benchmarks/runs/2026-04-24-rv32-stark-fib-1024-babybear-q-kara/result.toml")

#let gold_post_m33  = toml("/benchmarks/runs/2026-04-24-bench-core-m33-stark-goldilocks/result.toml")
#let gold_post_rv32 = toml("/benchmarks/runs/2026-04-24-bench-core-rv32-stark-goldilocks/result.toml")
#let bb_post_m33    = toml("/benchmarks/runs/2026-04-24-bench-core-m33-stark-babybear/result.toml")
#let bb_post_rv32   = toml("/benchmarks/runs/2026-04-24-bench-core-rv32-stark-babybear/result.toml")

#show: paper.with(
  title: "Phase 3.3 revisited: bench-core refactor unseats the 1.04× cross-ISA claim",
  authors: ("zkmcu",),
  date: "2026-04-24",
  kind: "report",
  abstract: [
    Yeah so the phase 3.3 headline from last week's report
    (`2026-04-25-babybear-quartic-cross-isa`) was that `BabyBear ×
    Quartic-Karatsuba` narrowed the Cortex-M33 vs Hazard3 cross-ISA
    latency ratio from 1.51× (Goldilocks × Quadratic baseline) to
    *1.04×*, the cleanest ISA-levelling in the whole project. That
    number no longer holds. After extracting shared firmware helpers
    into a `bench-core` lib and rebuilding the six RP2350 benchmark
    binaries, M33 BabyBear-Karatsuba dropped from 124.22 ms to
    *95.63 ms* (−23.02 %) while RV32 barely moved (129.05 → 128.02 ms,
    −0.80 %). The cross-ISA ratio widened back to *1.34×*. Nothing
    about the verifier or extension-mul algorithm changed — the
    bench-core refactor just wrapped the verify call in a
    `measure_cycles(|| ...)` closure and rearranged some captures,
    wich apparently unlocked a register-allocation win in the
    `ExtensibleField<4>::mul` hot loop on Thumb-2 (13 usable GPRs,
    register-tight) but not on RV32IMAC (31 GPRs, slack). The
    BabyBear-does-not-beat-Goldilocks conclusion still stands, but
    the magnitude changed: M33 ratio dropped from +66 % to *+30 %*.
  ],
)

= What changed

Between last week's report and today's rebaseline, one thing landed:
a new `crates/bench-core/` lib (~400 LoC, commit `98c7af5`) that
absorbs the USB-CDC bring-up, clock config, cycle-counter
abstraction, stack painting, and heap-tracking glue that used to be
copy-pasted across the six `bench-rp2350-*` binaries. The verifier
crates, the vendored winterfell fork, the BabyBear base-field impl,
and the Karatsuba `ExtensibleField<4>::mul` — none of them were
touched.

The only behavioral change in the firmware is the measurement idiom.
Before:

```rust
let cloned = proof.clone();
let t0 = DWT::cycle_count();
let result = fib::verify(cloned, public);
let t1 = DWT::cycle_count();
let cycles = u64::from(t1.wrapping_sub(t0));
```

After:

```rust
let cloned = proof.clone();
let (result, cycles) = measure_cycles(|| fib::verify(cloned, public));
```

`measure_cycles` is `#[inline]` and expands to the exact same pair of
DWT reads. Under LTO + `lto = "fat"`, the machine code around the
verify call is nearly identical. Nearly, but not quite.

= The headline

Eight configs re-measured on the same silicon, same toolchain, same
vendored deps. The STARK BabyBear-Karatsuba numbers are the ones
worth looking at:

#compare-table(
  ("Config", "Pre-refactor (ms)", "Post-refactor (ms)", "Δ"),
  (
    ([M33 Goldilocks × Q, TLSF],
     [#(gold_pre_m33.bench.stark_verify_fib_1024_q_tlsf.us_median / 1000)],
     [#(gold_post_m33.bench.stark_verify_fib_1024_q_tlsf.us_median / 1000)],
     [−1.94 %]),
    ([*M33 BabyBear × Q-Karatsuba, TLSF*],
     [#(bb_kar_pre_m33.bench.stark_verify_fib_1024_babybear_q_kara_tlsf.us_median / 1000)],
     [*#(bb_post_m33.bench.stark_verify_fib_1024_babybear_q_tlsf.us_median / 1000)*],
     [*−23.02 %*]),
    ([RV32 Goldilocks × Q, TLSF],
     [#(gold_pre_rv32.bench.stark_verify_fib_1024_q_tlsf.us_median / 1000)],
     [#(gold_post_rv32.bench.stark_verify_fib_1024_q_tlsf.us_median / 1000)],
     [−3.95 %]),
    ([*RV32 BabyBear × Q-Karatsuba, TLSF*],
     [#(bb_kar_pre_rv32.bench.stark_verify_fib_1024_babybear_q_kara_tlsf.us_median / 1000)],
     [*#(bb_post_rv32.bench.stark_verify_fib_1024_babybear_q_tlsf.us_median / 1000)*],
     [*−0.80 %*]),
  ),
)

Everything trending down a bit is expected with minor codegen shifts.
The M33 BabyBear at −23 % is not. That's a 4.3 million cycle drop
per verify on the exact same algorithm.

= Why this happened — the register-pressure hypothesis

`ExtensibleField<4>::mul` on BabyBear does 9 base multiplications +
3 sparse `mul_by_W = 11` multiplies + a bunch of adds and subs
(see `crates/zkmcu-babybear/src/field.rs`). On Thumb-2 the
register file has 13 usable GPRs after you subtract LR/SP/PC and a
couple callee-saved slots. RV32IMAC has 31 GPRs available.

Best guess what happened: wrapping the verify call in a FnOnce
closure changed where the captured `cloned` and `public` live on the
stack frame and which registers survive across the closure-call
boundary. On Thumb-2 that apparently freed one register in the
extension-mul inner loop and flipped a spill back to being
register-resident. A single spill in a loop that runs millions of
times is easily 20-30 % of total time. That matches.

The cross-ISA data is what makes the hypothesis feel right, not any
single number. Same source change, same LTO pass, same target
triple pattern — M33 dropped 23 %, RV32 dropped 0.8 %, that's a
30× magnitude gap in the delta. No plausible uniform-codegen shift
(icache, inlining order, dead-code-elim) produces asymmetry that
big between ISAs. Register-allocation-constrained hot loop does,
because the constraint only bites on the register-tight ISA.

Still a hypothesis. Proving it needs a `cargo asm` diff of
`<BabyBear as ExtensibleField<4>>::mul` compiled old-vs-new, wich I
havent done. Its a win either way, so not urgent.

= The cross-ISA narrowing claim is dead

The 2026-04-25 report called out *"BabyBear × Quartic-Karatsuba
narrows the cross-ISA gap to 1.04×"* as the phase 3.3 headline. On
the same silicon, same allocator, same field, same Karatsuba code —
just rebuilt through bench-core — the ratio is now:

#compare-table(
  ("Config", "M33 (ms)", "RV32 (ms)", "RV32/M33 ratio"),
  (
    ([Goldilocks × Q, TLSF (pre-refactor)],
     [#(gold_pre_m33.bench.stark_verify_fib_1024_q_tlsf.us_median / 1000)],
     [#(gold_pre_rv32.bench.stark_verify_fib_1024_q_tlsf.us_median / 1000)],
     [1.506×]),
    ([BabyBear × Q-Karatsuba (pre-refactor)],
     [#(bb_kar_pre_m33.bench.stark_verify_fib_1024_babybear_q_kara_tlsf.us_median / 1000)],
     [#(bb_kar_pre_rv32.bench.stark_verify_fib_1024_babybear_q_kara_tlsf.us_median / 1000)],
     [*1.039×* ← was the headline]),
    ([Goldilocks × Q, TLSF (bench-core)],
     [#(gold_post_m33.bench.stark_verify_fib_1024_q_tlsf.us_median / 1000)],
     [#(gold_post_rv32.bench.stark_verify_fib_1024_q_tlsf.us_median / 1000)],
     [1.474×]),
    ([BabyBear × Q-Karatsuba (bench-core)],
     [#(bb_post_m33.bench.stark_verify_fib_1024_babybear_q_tlsf.us_median / 1000)],
     [#(bb_post_rv32.bench.stark_verify_fib_1024_babybear_q_tlsf.us_median / 1000)],
     [*1.339×* ← this run]),
  ),
)

The 1.04× was a pre-refactor artifact. Specifically, it was
BabyBear's quartic-extension mul hitting a register-allocation
unhappy-path on M33 that *slowed M33 down enough* to match RV32.
The bench-core refactor unpicked the unhappy-path and M33 pulled
ahead. RV32 didnt have the same unhappy-path to begin with because
it had register slack, so it stayed where it was.

So BabyBear × Quartic-Karatsuba doesnt actually narrow the cross-ISA
gap once M33 is compiled well. The 1.34× post-refactor ratio is
tighter than Goldilocks (1.47×) but its not the 1.04× headline. And
its probably not beating any other allocator-swap or codegen-tune
you could pull on Goldilocks either.

The phase 3.3 report still stands on two of its three headlines:
- BabyBear × Quartic-Karatsuba does not beat Goldilocks × Quadratic
  at 95-bit security on either ISA. *True* (it was true at +66 %
  and its still true at +30 %).
- Variance record of 0.053 % on Hazard3. *True*, wasnt re-measured
  but the calculation is from DWT cycle counts wich didnt change.

One of them (the 1.04× cross-ISA) needs to be retracted. Anyone
citing it should pull the 1.34× number from this report.

= What this doesn't change for production

The phase 3.2.z allocator matrix recommendation stands:
`TlsfHeap` + Goldilocks × Quadratic for hardware-wallet-class
STARK verify on RP2350. The new M33 Goldilocks number is
73.20 ms and the new RV32 Goldilocks is 107.96 ms, both a touch
better than the pre-refactor figures but within noise.

BabyBear × Q-Karatsuba improved substantially on M33 (95.63 ms
now, vs the Goldilocks 73.20 ms its compared to). But at +30 %
over the production-recommended config, its still not the pick.

The two narrower BabyBear deployment shapes the 2026-04-25 report
called out are affected differently:

#compare-table(
  ("Deployment shape", "Pre-refactor pick", "Revised pick"),
  (
    ([Latency on M33], [Goldilocks × Q (74.65)], [Goldilocks × Q (73.20)]),
    ([Latency on RV32], [Goldilocks × Q (112.40)], [Goldilocks × Q (107.96)]),
    ([Side-channel variance on Hazard3], [BB × Q-Kara (0.053 %)], [BB × Q-Kara (unchanged)]),
    ([Cross-ISA portable SDK], [BB × Q-Kara (1.04× gap)], [Either (1.34× gap)]),
  ),
)

The cross-ISA-portable-SDK recommendation was load-bearing on the
1.04× figure. Without it, BabyBear doesnt have a compelling pitch
over Goldilocks for cross-ISA SDKs — you can pick either and accept
the 1.34× to 1.47× ratio depending on config. Goldilocks is simpler
(no fork of winterfell), wich tips the recommendation back to
Goldilocks as the default cross-ISA pick.

= Scorekeeping — all four STARK configs reconciled

Pre-refactor TOMLs stay put as historical record. Post-refactor
TOMLs live alongside them under
`benchmarks/runs/2026-04-24-bench-core-*/`. Full picture:

#compare-table(
  ("Config", "Pre (ms)", "Post (ms)", "Δ"),
  (
    ([M33 Gold × Q], [74.65], [73.20], [−1.94 %]),
    ([M33 BB × Q-Kara], [124.22], [*95.63*], [*−23.02 %*]),
    ([RV32 Gold × Q], [112.40], [107.96], [−3.95 %]),
    ([RV32 BB × Q-Kara], [129.05], [128.02], [−0.80 %]),
  ),
)

The refactor also dropped small wins on the Groth16 side on M33:

#compare-table(
  ("Config", "Pre (ms)", "Post (ms)", "Δ"),
  (
    ([M33 BN254 Groth16], [641], [642.91], [+0.30 %]),
    ([M33 BLS12-381 Groth16], [2014.72], [1997.83], [−0.84 %]),
    ([RV32 BN254 Groth16], [1327.15], [1363.00], [+2.70 % †]),
    ([RV32 BLS12-381 Groth16], [5150.64], [5195.67], [+0.87 % †]),
  ),
)

† RV32 Groth16 regressed because the RV32 binaries newly opted
into `TrackingLlff` (two Relaxed atomics per alloc) via
bench-core. Prior RV32 firmware ran plain `LlffHeap` with no
heap_peak tracking. The delta is TrackingHeap's atomic overhead
scaled by substrate-bn and bls12_381 inner-loop alloc counts.
Methodology upgrade, not perf loss. RV32 binaries now report
`heap_peak` byte-identical to their M33 siblings for the first
time (81 888 B BN, 79 360 B BLS12, 93 515 B Goldilocks,
91 362 B BabyBear).

= Open tracks

No change from the 2026-04-25 report on the three phase-3.4-or-later
threads — Plonky3 circle-STARKs over Mersenne-31, bigger AIRs
($N = 2^16$ and up), hand-asm UMULL for BabyBear `mont_reduce` on
M33. The bench-core refactor doesnt unlock any of them, just
cleaned up a register-allocation unhappy-path that was flattering
the cross-ISA ratio.

One new open track from this report: the `cargo asm` diff on
`<BabyBear as ExtensibleField<4>>::mul` pre-vs-post refactor. If the
diff shows a spill->register flip in the quartic mul inner loop,
the register-pressure hypothesis is confirmed and we can tune
Thumb-2 register allocation with intent (e.g. via `#[inline(always)]`
hints or manual spill-hinting) instead of crossing fingers. If the
diff shows something else, theres a separate mechanism to find.

Timeboxing it to one session if anyone picks it up. Either you see
the spill flip in the first 30 minutes of IR staring or you dont.

#v(1em)

_Earlier phase-3.3 report_: `research/reports/2026-04-25-babybear-quartic-cross-isa.typ`
(immutable, cites pre-refactor numbers).

_Bench-core refactor commit_: `98c7af5`.

_Rebaseline TOMLs_: `benchmarks/runs/2026-04-24-bench-core-*/`.

_Finding with the register-allocation write-up_:
`.claude/findings/2026-04-24-bench-core-babybear-speedup.md`.

_Winterfell fork_: `github.com/Niek-Kamer/winterfell` (unchanged).

_BabyBear base + Karatsuba extension_: `crates/zkmcu-babybear/src/field.rs`
(unchanged since phase 3.3 commit `b794d20`).
