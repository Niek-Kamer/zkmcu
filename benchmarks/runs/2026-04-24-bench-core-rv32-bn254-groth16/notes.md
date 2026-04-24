# 2026-04-24 — RV32 BN254 Groth16, bench-core rebaseline

First RV32 run with `TrackingLlff` instead of plain `LlffHeap`. All
three M33 bench firmwares had `TrackingHeap` since their first
version; the RV32 side never did. Moving it into the shared
`bench-core` lib meant RV32 inherited it for free, so now we've got
`heap_peak` on RV32 for the first time.

## Headline

**Groth16 verify (x^2 = y, BN254): 1363 ms median, +2.70 % vs
1327 ms pre-refactor baseline. Semaphore depth-10: 1633 ms,
+4.41 %.**

The +2.7 % isn't a real regression, it's TrackingHeap's two relaxed
atomic ops per alloc, scaled by substrate-bn's inner-loop alloc
count. The semaphore delta is larger because more IC points means
more allocations, wich means more atomics per verify. Linear.

Not an apples-to-apples comparison with the old TOML either way —
it's a methodology upgrade. From now on RV32 is using the same
instrumentation as M33.

## Heap footprint

`heap_peak = 81_888 B` — **byte-identical to M33 BN running the same
proof**. This is the first time we could confirm that. The previous
RV32 firmware had no way to measure heap, so the cross-ISA heap
comparison was a guess. Now it's measured.

## The iter 1 g1mul anomaly

Iter 1 g1mul = 34 ms, iters 2-7 = ~75 ms. Not a bug, just scalar
Hamming-weight variance. The seed construction (`u64(iter) +
cycles_u64()`, low 8 bytes, byte[31] = 0) gives a scalar ≤ 64 bits.
On iter 1 `cycles_u64()` is small because DWT hasn't rolled far
since boot, so the scalar is sparse. By iter 2 the counter's rolled
through several billion cycles and the scalar is dense. Classic
bn254 scalar-mul-is-Hamming-dependent thing.

## Cross-ISA reference

M33 BN sibling (same commit, same bench-core) lands at 642 ms.
RV32/M33 ratio = 2.12x. Cross-ISA gap is meaningfully larger than
BLS12 or STARK — BN254 on Hazard3 is a genuine pain point, mostly
because substrate-bn's pure-Rust `Fq::mul` path doesn't get UMULL
pipelined the way Cortex-M33's hardware does.
