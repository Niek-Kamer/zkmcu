# 2026-04-29 rv32 PQ-Semaphore depth-10 — headline Plonky3 verify on Hazard3

## Headline

`pq_semaphore_verify`: **1249.59 ms** median across 21 iterations on
the Pico 2 W Hazard3 RV32IMAC @ 150 MHz. All iterations `ok=true`.
Variance **0.055 %** (max - min over median).

This is the headline phase-4.0 result on the RV32 side: same firmware
shape as the M33 measurement, same proof bytes, same audited
Poseidon2-16 constants. Replaces the Semaphore v4 BN254/Groth16
verifier with a PQ-secure STARK on the Hazard3 core.

## What this falsifies

Same scoping doc, RV32 row: predicted *940-1880 ms, point estimate
1250 ms*. Measured **1249 ms** — within rounding of the point
estimate. This is the cleanest match between a published prediction
and a measurement in the project to date.

| Quantity                 | Predicted (RV32)    | Measured          | Verdict          |
|--------------------------|---------------------|-------------------|------------------|
| Verify on RV32           | 940-1880 ms         | 1249.59 ms        | inside (point match) |
| Proof size               | 15-30 KB            | 165 KB            | over (5.5x high) |
| Heap (after parse)       | 80-140 KB           | 150 KB            | over (~7%)       |
| Variance                 | 0.05-0.15 %         | 0.055 %           | inside (lower edge) |
| Stack peak               | 4-8 KB              | not captured      | -                |

## What this means against Groth16/BN254 on RV32

RV32's BN254 verify runs at 1363 ms
(`benchmarks/runs/2026-04-28-rv32-bn254-rebench`), because Hazard3
lacks the M33's UMAAL multiply-accumulate that the BN254 U256::mul
asm depends on. PQ-Semaphore at 1249 ms is **1.09x faster than
Groth16 on this ISA** — the "PQ tax" inverts on RV32.

This is a substantively different headline from the M33 side. M33 is
where BN254 is well-tuned and PQ pays a 1.91x cost; RV32 is where
BN254 is under-tuned and PQ is actually a net speedup. The cross-ISA
gap drops from 2.20x (BN254 RV32 vs M33) to 1.19x (PQ-Semaphore RV32
vs M33), which is the real story for cross-ISA portability of the PQ
path: the verify cost is dominated by Poseidon2-16 + FRI work, not by
big-integer field arithmetic, so the ISAs converge.

## Cross-ISA pattern

| Workload                  | M33 ms   | RV32 ms  | Ratio  |
|---------------------------|----------|----------|--------|
| Groth16/BN254 verify      | 550.67   | 1363.23  | 2.20x  |
| Poseidon-chain (Plonky3)  | 492.22   | 615.87   | 1.252x |
| **PQ-Semaphore (Plonky3)**| 1049.72  | 1249.59  | **1.190x** |

Tighter ratio on the headline than on the chain anchor. Possible
explanation: the headline AIR's heavier constraint evaluation
(Merkle + scope + nullifier) is plain BabyBear arithmetic which
Hazard3 handles essentially as well as M33; the chain anchor is
more Poseidon2-bound, where M33's slightly better cache and pipeline
behaviour shows up more.

## Capture issue worth flagging

Same as M33: boot line + `[fp] before parse_proof` +
`[fp] after parse_proof` clean, then write_line's 1 s deadline
expired during `make_config` / `build_air` / `boot_measure`, losing
the `[boot]` line that carries `stack_peak` and final `heap_peak`.

`heap_after_parse = 154 056 B` is byte-identical to M33 — same
postcard decode of the same proof bytes, both ISAs use the same
heapless-style allocator pattern. `heap_peak` and `stack_peak` not
captured this run; expected ~250-280 KB and ~3-5 KB respectively.

## Surprises worth flagging

1. **Predicted/measured match within 0.08 %.** Predicted 1250 ms,
   measured 1249.59 ms. The scoping doc's RV32 prediction was driven
   by applying a 1.04x cross-ISA factor (from phase 3.3 BabyBear x
   Quartic) to the M33 prediction. That factor is ~14 % off the
   actual 1.19x ratio measured here, but the M33 prediction itself
   was high-side, so the errors cancel and the RV32 number lands
   on point. Lucky, but worth flagging — the published RV32
   prediction is more right by accident than by analysis.

2. **Variance 0.055 % vs M33 0.029 %.** Hazard3 is consistently a
   touch noisier than M33 across every workload measured (chain
   anchor: 0.076 % vs 0.051 %; PQ-Semaphore: 0.055 % vs 0.029 %).
   No theory yet for why; possibly cache-line behaviour around the
   global `HEAP` allocator or interrupt jitter from the timer that
   isn't visible in M33's DWT-based path.

## Reproducibility

```bash
just regen-vectors

just build-rv32-pq-semaphore

scp target/riscv32imac-unknown-none-elf/release/bench-rp2350-rv32-pq-semaphore \
    pid-admin@10.42.0.30:/tmp/bench-rv32-pq-semaphore.elf

# Pi 5, Pico in BOOTSEL:
picotool load -v -x -t elf /tmp/bench-rv32-pq-semaphore.elf
cat /dev/ttyACM0
```

## Files

- `raw.log` — captured serial output, 21 + 3 iterations across two flashes.
- `result.toml` — structured results + falsification scorecard.
- This `notes.md`.
