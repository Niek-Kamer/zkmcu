# 2026-04-24 — RV32 STARK BabyBear x Quartic, bench-core rebaseline

Same winterfell fork (Quartic extension added in
`Niek-Kamer/winterfell`), same zkmcu-babybear base-field impl,
just rebuilt against `bench-core`.

## Headline

**stark_verify (Fibonacci N=1024, BabyBear x Quartic, schoolbook
16-mult): 128.02 ms median, −6.31 % vs 136.64 ms pre-refactor.**

Solid but boring compared to the M33 BabyBear result (−23.0 % on
same refactor). The cross-ISA asymmetry is the real story here, not
the -6 %.

## The register-allocation hypothesis, strengthened

If the bench-core refactor had caused a global codegen shift
(function layout, icache, inlining), both ISAs would have moved by
similar magnitudes. The fact that M33 BabyBear dropped 23 % and RV32
BabyBear dropped only 6 % on the same source code change — that
asymmetry is what a register-pressure-specific effect looks like.

Thumb-2 has ~13 usable GPRs, RV32IMAC has 31. The quartic-extension
mul is at the edge of spilling on Thumb-2 and probably wasn't
spilling at all on RV32. So when the closure-wrap shifted where
captures live, M33 picked up a big win and RV32 picked up whatever
the baseline codegen improvements were (better dead-code elim in
the call graph, maybe some minor inlining win).

## Phase 3.3 ratios

BabyBear x Quartic vs Goldilocks x Quadratic on RV32: **1.186x
(+18.6 %)**, down from the pre-refactor 1.217x (+21.7 %). Also
down from the original +15 % figure with Karatsuba — but Karatsuba
isn't re-measured yet.

Cross-ISA: RV32 BabyBear / M33 BabyBear = 128.02 / 95.63 = 1.339x.
That's *wider* than the pre-refactor ratio (136.64 / 124.21 =
1.100x) because M33 got the bigger refactor win. The "BabyBear
narrows the cross-ISA gap to 1.04x" claim from the phase 3.3 memo
was about the Karatsuba variant specifically and needs re-checking
under bench-core.

## Heap + variance

`heap_peak = 91_362 B`, byte-identical to M33 sibling. First time
measured. Variance (max-min)/median = 0.23 %, tight.
