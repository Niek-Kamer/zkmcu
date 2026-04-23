# 2026-04-23 — Hazard3 RV32 STARK Fibonacci verify

Companion to `2026-04-23-m33-stark-fib-1024`. Same silicon, same clock,
same firmware shape, same proof bytes — Hazard3 RV32 core instead of
Cortex-M33. First RV32 STARK verify measurement.

## Headline

**64.1 ms** median verify over 16 iterations. Iteration-to-iteration
variance *0.69 %*. Every iteration `ok=true`.

## Cross-ISA: RV32 is 1.46x M33 on STARK verify

| Operation | Cortex-M33 | Hazard3 RV32 | RV32 / M33 |
|---|---:|---:|---:|
| STARK Fibonacci verify | 43.8 ms | 64.1 ms | **1.46x** |
| Peak stack | 5.2 KB | 5.1 KB | ≈ |
| Variance | 0.30 % | 0.69 % | — |

Predicted RV32/M33 ratio was *0.7 - 1.3x*. Measured 1.46x — **outside
the predicted band**, third falsification criterion from the phase-3.1
prediction report fires.

This pattern matches the BLS12-381 finding from phase 2.6: M33
consistently wins on hash + field-arithmetic-heavy workloads at larger
primes, and the original "Hazard3 beats M33 on G1 scalar mul at BN254"
result (which itself went away after a `substrate-bn` dep bump) does
NOT generalise.

Per-component contribution to the RV32/M33 gap, best-guess without
disassembly diff:

- *Blake3 inner loop*: 32-bit ADD/XOR/ROT in tight unrolled sequences.
  Cortex-M33 Thumb-2 encoding is denser than RV32IMAC's instruction
  stream here, better instruction-cache pressure. Plausibly a 10-20 %
  gap on this component.
- *Goldilocks mul*: 64-bit schoolbook on 32-bit hardware.
  Cortex-M33's `UMULL` gives the 32x32=64 product in one op; Hazard3
  needs `mul` + `mulhu` as two separate instructions. Same pattern
  caused the BLS12-381 G1 mul gap.
- *Merkle auth-path traversal*: tight loop with Blake3 hash call
  inside. Whatever hits the Blake3 per-op overhead gets compounded
  13 layers deep x 32 queries = ~400 iterations.

All three factors compound into the 46 % RV32/M33 gap. Not a
catastrophic finding, just a continued piece of evidence that
cross-ISA conclusions are workload-sensitive and need measurement,
not extrapolation.

## Memory

- `stack_peak` = 5,096 bytes. Matches M33 within 2 %, as expected —
  stack is determined by verify logic not ISA.
- `heap_peak` not captured (RV32 firmware uses plain `LlffHeap` without
  the `TrackingHeap` wrapper). Expected peak ~70 KB based on M33 run,
  since winterfell's allocations don't depend on ISA. Porting
  `TrackingHeap` to RV32 is a phase-3.2 follow-up.

## Variance anomaly

0.69 % vs predicted 0.03-0.1 %. Higher than M33's 0.30 %, about 7x
the upper end of the predicted band. Same `proof.clone()`-inside-the-
timed-window hypothesis applies. RV32 allocator might have slightly
different jitter characteristics (different object layout, different
code paths in the allocator due to struct padding).

Factor of 2 higher variance than M33 is worth noting — could be:

1. Hazard3's in-order pipeline having a different jitter profile for
   allocator work
2. `LlffHeap` (no TrackingHeap wrapper) having slightly different
   fast-path / slow-path behaviour
3. Just sample-size noise — 16 samples vs 17, both small

Investigating further is phase-3.2 cleanup work, not a blocker.

## Reproduction

```bash
cargo build -p bench-rp2350-rv32-stark --release
scp target/riscv32imac-unknown-none-elf/release/bench-rp2350-rv32-stark \
    pid-admin@10.42.0.30:/tmp/bench-rv32-stark.elf

# Pi 5, Pico in BOOTSEL:
picotool load -v -x -t elf /tmp/bench-rv32-stark.elf
cat /dev/ttyACM0
```
