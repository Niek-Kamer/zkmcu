# 2026-04-24 — Hazard3 RV32 STARK Fibonacci verify, Quadratic extension

Companion to `2026-04-24-m33-stark-fib-1024-q`. Same silicon, same clock,
same firmware shape, same proof bytes — RV32 Hazard3 core at 150 MHz.
First production-grade-security STARK verify on the RISC-V side.

## Headline

**93.0 ms** median verify over 18 iterations. Iteration-to-iteration
variance *0.29 %*, actually *better* than phase 3.1 RV32's 0.69 %.
Every iteration `ok=true`.

## Cross-ISA: ratio narrowed under Quadratic

| Configuration | M33 | RV32 | RV32/M33 |
|---|---:|---:|---:|
| Phase 3.1 (FieldExtension::None, 63-bit) | 43.8 ms | 64.1 ms | **1.46×** |
| Phase 3.2 (FieldExtension::Quadratic, 95-bit) | 70.0 ms | 93.0 ms | **1.33×** |

Phase 3.2.0 predicted the ratio would *widen* to 1.45–1.65× because
extension-field arithmetic was supposed to amplify M33's UMULL / register
advantage. It narrowed instead. Falsification.

Best hypothesis: the Quadratic proof is more auth-path-dominated
(hash bytes unchanged from `None`) and less FRI-fold-arithmetic-dominated
(the part Quadratic amplifies). The cross-ISA gap on hashing is smaller
than on 64-bit field muls, so shifting the workload mix toward hashing
narrows the gap.

This is consistent with the proof-size finding from phase 3.2.1:
serialised proof only grew 22 % (25.3 → 30.9 KB) because auth paths
dominate, not the 70 %+ growth that a "half the proof is FRI evals"
model would predict.

## Variance finding

Phase 3.2.0 predicted RV32 variance 0.5–1.2 %, slightly worse than
phase 3.1's 0.69 %. Measured *0.29 %* — **better** than phase 3.1.

This falsifies both the phase 3.2.0 prediction *and* the
`proof.clone()`-inside-timed-window hypothesis from phase 3.1, which
implied bigger proof → bigger clone → more allocator jitter.

Possible explanation: allocator jitter is roughly fixed-cost per
allocation event rather than per-byte. A longer measurement window
(93 ms vs 64 ms) spreads the same fixed jitter cost over more total
time, reducing the fractional variance.

Either way, the measurement is tighter than expected — publishable
as a clean number regardless of the underlying mechanism.

## Memory

- `stack_peak` = 5,528 B. Up from 5,096 B under `None` (+8 %),
  matching M33's 5,640 B under Quadratic. Within predicted 5.2–7 KB band.
- `heap_peak` not captured — RV32 firmware still uses plain `LlffHeap`
  without `TrackingHeap`. Expected peak ~93 KB based on the M33 run,
  keeping total RAM under the 128 KB tier by comfortable margin.

The `TrackingHeap` port to RV32 is still phase-3.2 cleanup work.

## vs prediction (phase 3.2.0)

| | Predicted | Measured | Δ |
|-|---:|---:|---|
| Verify time | 130–210 ms | 93.0 ms | **37 ms below band** |
| Stack peak | 5.2–7 KB | 5.5 KB | within |
| Variance | 0.5–1.2 % | 0.29 % | **below band** |
| Cross-ISA ratio | 1.45–1.65× | 1.33× | **below band** |

Three of four RV32 predictions fell below the predicted band. Same
optimistic-miss pattern as M33 under Quadratic, and as both ISAs under
phase 3.1.

## Reproduction

```bash
cargo build -p bench-rp2350-rv32-stark --release
scp target/riscv32imac-unknown-none-elf/release/bench-rp2350-rv32-stark \
    pid-admin@10.42.0.30:/tmp/rv32q.elf

# Pi 5, Pico in BOOTSEL:
picotool load -v -x -t elf /tmp/rv32q.elf
cat /dev/ttyACM0
```
