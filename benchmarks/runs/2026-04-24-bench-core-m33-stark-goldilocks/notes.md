# 2026-04-24 — M33 STARK Goldilocks x Quadratic, bench-core rebaseline

Same vendored winterfell, same TLSF allocator, same AIR. Rebuilt after
the `bench-core` refactor landed (commit `98c7af5`).

## Headline

**stark_verify (Fibonacci N=1024, Goldilocks x Quadratic):
73.2 ms median, −1.9 % vs the 74.6 ms pre-refactor TLSF baseline.**

Within noise. The interesting M33 STARK result is on the BabyBear
sibling (see `../2026-04-24-bench-core-m33-stark-babybear/`), not
here. Goldilocks's Quadratic extension has register slack on Thumb-2
and the bench-core closure wrapping didn't shift anything meaningful.

## Everything else matches

heap_peak 93515 B identical, stack +248 B (closure frame), variance
0.32 % (same as prior TLSF). Result `ok=true` every iteration.

## First 4 iters missing from serial

The captured log shows iters 5-20 because the first 4 were lost to
the usual post-long-call USB host race. Not a regression, just the
same USB quirk we see on every flash, 16 samples is still plenty
for a stable median.
