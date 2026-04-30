# Phase D — Goldilocks × Quadratic verify on Hazard3 (RV32IMAC)

Companion to `2026-04-29-m33-pq-semaphore-gl/`. Same proof bytes, same
loop, only the firmware target changes.

## Stats

- 20 iterations.
- Honest verify: 2700.84 ms median, 0.040 % range.
- Heap peak: 415 312 B (same as M33, allocator state is target-
  independent).
- All 20 iters returned `ok=true`.

## Cross-ISA cost

| Phase | M33 (ms) | RV32 (ms) | RV32 / M33 |
|---|---:|---:|---:|
| A (BB grind only)         | 1051.09 | 1255.98 | 1.195 |
| B (BB d6 + grind)         | 1065.84 | 1269.73 | 1.191 |
| C (BB full pipeline)      | 1130.58 | 1302.64 | 1.152 |
| D (GL × Quadratic)        | 1995.66 | 2700.84 | **1.354** |

Phase D widens the cross-ISA gap. The BabyBear configs sit at ~1.19×;
Phase D jumps to 1.354× because Goldilocks's 64-bit modular reduction
is precisely the case where the M33's UMAAL (`32×32+32+32→64`) shows
the biggest win. Hazard3 has no fused multiply-accumulate of that
shape, so it pays the full multi-precision cost on every Goldilocks
mul.

For the portability story: BabyBear × Quartic delivers a tight 1.19×
ratio across ARMv8-M and RV32 — devices feel the same. Goldilocks ×
Quadratic regresses to 1.35× — pick your CPU more carefully if you go
that route.

## Hypothesis verdict

The plan predicted Goldilocks × Quadratic to drop RV32 verify from
1269.73 ms → 1100–1300 ms (basically flat or slightly faster). Measured
2700.84 ms — **+113 %** over baseline, far outside the predicted band.
Same root cause as the M33 sibling (PQ-Semaphore verify is hash-bound,
not arithmetic-bound), amplified on Hazard3 by the missing UMAAL.

## Outstanding

- Stack peak not captured. `measure_stack_peak`'s 64 KB sentinel paint
  doesn't fit alongside a 480 KB heap; same caveat as the M33 GL run
  and the Phase C reject runs.

## Links

- Plan section: `bindings/.claude/plans/2026-04-29-security-128bit.md` § Phase D
- M33 sibling: `benchmarks/runs/2026-04-29-m33-pq-semaphore-gl/`
- Phase B BB-d6 baseline (RV32): `benchmarks/runs/2026-04-29-rv32-pq-semaphore-d6/`
- Verifier module: `crates/zkmcu-verifier-plonky3/src/pq_semaphore_goldilocks.rs`
- Firmware: `crates/bench-rp2350-rv32-pq-semaphore-gl/`
