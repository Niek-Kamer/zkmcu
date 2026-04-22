# 2026-04-22 — Hazard3 RV32 Semaphore depth-10 verify

Companion to `2026-04-22-m33-semaphore-depth10` — same silicon, same
clock, same vectors, running on the RP2350's RISC-V core instead of
the ARM core. First RV32 measurement of a production-shaped Groth16
circuit.

## Headline

*Semaphore verify: 1564 ms* on the Hazard3 core at 150 MHz, variance
**0.030 %** over 6 iterations. Every iteration `ok=true`.

## Prediction check

The phase 2.7 response budget predicted RV32 ~1620 ms, noting the
prediction was shaky because it extrapolated from BN254 small-scalar
behaviour. Measured: *1564 ms*, off by −3.5 %. Within the uncertainty
band.

## Cross-ISA comparison on Semaphore vs square

| | square | Semaphore | Δ |
|-|-------:|----------:|--:|
| M33 | 962 ms | 1176 ms | +214 ms |
| RV32 | 1327 ms | 1564 ms | +237 ms |
| RV32 / M33 | 1.38× | **1.33×** | — |

The ratio tightens from 1.38× on square to 1.33× on Semaphore. Phase 2.6's
BLS12-381 comparison report flagged the pattern: *Hazard3 wins on G1
scalar mul for BN254*, so any circuit that adds G1 scalar muls to the
verify path (i.e. big-scalar public inputs, wich Semaphore has) pulls
RV32 proportionally closer to M33. This is an *independent* datapoint
confirming that finding — we didn't have to contrive a test for it.

Per-extra-public-input cost:

| | ms per big-scalar input |
|-|--:|
| M33 | 71 |
| RV32 | 79 |

RV32 loses ~11 % more per extra input. That's the opposite of the
standalone G1-mul ratio (RV32 wins 35 % on isolated G1 mul on BN254).
Gap closes because Semaphore's scalar mul includes the point addition
into the IC accumulator and the accumulator logic on RV32 pays a small
tax. Not catastrophic.

## Limitations

- *heap_peak not captured* (RV32 firmware uses `LlffHeap` without the
  `TrackingHeap` wrapper the M33 firmware has). Expected to match the
  M33 peak (~81 KB) since the allocations come from the same verifier
  code; porting TrackingHeap to RV32 is a phase-3 follow-up.
- *.text / .data / .bss not extracted* — same reason as the M33 run.
- *Iteration 7 truncated* by Ctrl+C (expected; verify_sq5 wasn't even
  run in the loop on RV32 firmware, just verify_square + verify_semaphore).

## Reproduction

```bash
cargo build -p bench-rp2350-rv32 --release
scp target/riscv32imac-unknown-none-elf/release/bench-rp2350-rv32 \
    pid-admin@10.42.0.30:/tmp/bench-rv32.elf

# Pi 5, Pico in BOOTSEL:
picotool load -v -x -t elf /tmp/bench-rv32.elf
cat /dev/ttyACM0
```
