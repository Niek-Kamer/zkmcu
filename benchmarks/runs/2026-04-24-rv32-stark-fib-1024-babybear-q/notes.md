# 2026-04-24 — Hazard3 RV32 STARK BabyBear + Quartic, first measurement

Companion to `2026-04-24-m33-stark-fib-1024-babybear-q`. Same silicon,
same AIR (Fib-1024), same hasher (Blake3-256), same allocator (TLSF),
same ProofOptions (32 queries, blowup 8, 0 grinding, FRI folding 8) as
phase 3.2.z; only the field + extension degree changes.

## Headline

**136.64 ms median**, 0.058 % std-dev variance, 10 clean iterations.
**+21.6 % vs Goldilocks-Quadratic-TLSF** (112.40 ms) on the same silicon.

Compare the M33 side: +66 % regression. *The same field swap hurts Hazard3
four times less than it hurts Cortex-M33.*

## Why the M33 vs RV32 asymmetry

Hazard3 is u32-native and pays a real tax on `u64` arithmetic: every 64-bit
multiply expands to a `MUL` + `MULHU` pair plus carry handling. Goldilocks'
`mont_red_cst` uses `u128` arithmetic in hot loops, wich compounds that cost.
BabyBear's `mont_reduce` is a single `u32 × u32 → u64` multiply plus a
conditional subtract — the exact shape Hazard3 is built for.

Cortex-M33, on the other hand, has a beefier 32-bit multiplier with
`UMULL`/`UMAAL` and a pipeline that tolerates `u64` arithmetic reasonably
well. Goldilocks on M33 therefore pays a smaller `u64` tax than Hazard3
does, and BabyBear has less to reclaim.

The net: the M33-vs-RV32 cross-ISA gap collapses from 1.506× (Goldilocks-Q)
to 1.100× (BabyBear-Q). That is the single biggest ISA-leveling move we've
seen — bigger than any allocator swap in phase 3.2.z.

## Variance

`std_dev / mean = 0.058 %` is the tightest variance in the project to
date, below BumpAlloc's 0.076 % on this same ISA. Likely cause: BabyBear
base-field arithmetic is branch-free on u32-native Hazard3, so the hot
path has fewer mispredict opportunities than Goldilocks' emulated u64.

This makes BabyBear interesting for variance-sensitive applications
(side-channel-resistant firmware) in addition to its ISA-leveling
property.

## Heap not directly measured

This RV32 firmware variant does not run the `TrackingHeap` wrapper that
the M33 firmware does, so `heap_peak` isn't in the serial dump. The
zkmcu-host-gen Quartic proof is byte-identical for both ISAs (same
winterfell prover, same AIR, same options), so the allocation trace is
the same too; heap peak should match the M33 reading at 91_362 B, well
within the 128 KB tier.

## Reproduction

```bash
cargo run -p zkmcu-host-gen --release -- stark-babybear
just build-rv32-stark-bb

scp target/riscv32imac-unknown-none-elf/release/bench-rp2350-rv32-stark \
    pid-admin@10.42.0.30:/tmp/bench-rv32-stark-bb.elf

# Pi 5 after BOOTSEL:
picotool load -v -x -t elf /tmp/bench-rv32-stark-bb.elf
cat /dev/ttyACM0
```
