# 2026-04-24 — M33 STARK BabyBear + Quartic, first measurement

First number on the `Niek-Kamer/winterfell` fork's `FieldExtension::Quartic`
path with `zkmcu-babybear` as the base field. Same allocator (TLSF), same
AIR (Fibonacci N=1024), same hasher (Blake3-256), same proof-options knobs
(32 queries, blowup 8, 0 grinding, FRI folding 8) as phase 3.2.z, so the
comparison is genuinely single-variable (field+extension).

## Headline

**124.21 ms median**, 0.082 % std-dev variance, 15 clean iterations.
**+66 % vs Goldilocks-Quadratic-TLSF** (74.65 ms) on the same silicon.

## Why the regression

Goldilocks × Quadratic costs 3 base multiplications per extension multiply
(Karatsuba in `winter-math::f64::ExtensibleField<2>::mul`). BabyBear ×
Quartic with schoolbook convolution costs 16 base multiplications. Even if
BabyBear's base multiply is ~3-4× faster than Goldilocks' (single UMULL vs
emulated `u64 × u64 → u128`), the 16/3 ≈ 5.3× extension-arithmetic inflation
dominates.

Two unoptimised paths contribute to the 16-mult count:

1. `ExtensibleField<4>::mul` on BabyBear uses schoolbook 4x4 convolution.
   Karatsuba-style brings this to 9 base mults. See
   `crates/zkmcu-babybear/src/field.rs::ExtensibleField<4>::mul`.
2. Three `w * (...)` multiplications inside `mul` use a full Montgomery
   reduction when W = 11 is actually `(x << 3) + (x << 1) + x` — two shifts
   and two adds, no reduction required until the base-field add handles it.

Both are headroom left on the table for the Karatsuba-follow-up run.

## Heap and stack are fine

- `heap_peak = 91_362 B` (vs Goldilocks-Quadratic-TLSF 93_515 B). Fits the
  128 KB production tier with 34 KB margin.
- `stack_peak = 5_264 B` (vs Goldilocks ~5_632 B). Slightly smaller because
  a base BabyBear element is 4 B where Goldilocks is 8 B, and some of the
  AIR's local arithmetic parks base elements on the stack.

## Variance

`std_dev / mean = 0.082 %`, essentially identical to the Goldilocks TLSF
baseline (0.081 %). The fork + BabyBear + Quartic did not introduce any
new allocator-driven timing noise. The regression is purely compute.

## Reproduction

```bash
# Workspace root:
cargo run -p zkmcu-host-gen --release -- stark-babybear
just build-m33-stark-bb

# Dev machine -> Pi 5:
scp target/thumbv8m.main-none-eabihf/release/bench-rp2350-m33-stark \
    pid-admin@10.42.0.30:/tmp/bench-m33-stark-bb.elf

# Pi 5 after BOOTSEL:
picotool load -v -x -t elf /tmp/bench-m33-stark-bb.elf
cat /dev/ttyACM0
```
