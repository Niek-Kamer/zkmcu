# 2026-04-24 — RV32 STARK Goldilocks x Quadratic, bench-core rebaseline

Same vendored winterfell, same AIR, TLSF allocator now via
`TrackingTlsf` from `bench-core`.

## Headline

**stark_verify (Fibonacci N=1024, Goldilocks x Quadratic):
107.96 ms median, −3.95 % vs 112.40 ms pre-refactor TLSF baseline.**

Small speedup on the STARK side despite TrackingTlsf's added atomics
overhead. STARK verify does far fewer allocations than BN or BLS12
Groth16 (most field-element intermediates sit on the stack), so
TrackingHeap's per-alloc cost is dwarfed by whatever codegen shift
the bench-core call-graph change produced.

Same direction as M33 Goldilocks (-1.9 %) and M33 BabyBear (-23.0 %)
and RV32 BabyBear (-6.2 %). All four STARK configs got small-to-big
wins from the refactor; no STARK config regressed.

## Heap confirmation

`heap_peak = 93_515 B` — **byte-identical to M33 Goldilocks**. First
time measured on RV32.

## Cross-ISA reference

M33 Goldilocks sibling (same commit) = 73.2 ms. RV32/M33 = 1.47x.
Slight improvement on the 1.51x pre-refactor ratio, but Goldilocks
STARK remains the config where Hazard3 is closest to M33.
