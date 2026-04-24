# 2026-04-24 — Hazard3 RV32 STARK BabyBear + Quartic, Karatsuba extension mul

Companion to `2026-04-24-m33-stark-fib-1024-babybear-q-kara`. Same code
change (9-mult Karatsuba `ExtensibleField<4>::mul` + 3 sparse `mul_by_W`
doublings for `W = 11`). Exercises the Hazard3 in-order pipeline where
the extra Mont-muls cost full cycles vs Cortex-M33's pipelined UMULL.

## Headline

**129.05 ms median**, **0.053 % std-dev variance** (tightest in the
project to date), 17 clean iterations. **Delta vs schoolbook: -7.59 ms
(-5.55 %).** Karatsuba works here where it didn't on M33.

## The cross-ISA story now

| ISA | Goldilocks-Q | BabyBear-Q schoolbook | BabyBear-Q Karatsuba |
|---|---:|---:|---:|
| Cortex-M33 | 74.65 ms | 124.21 ms | 124.22 ms |
| Hazard3 RV32 | 112.40 ms | 136.64 ms | 129.05 ms |
| RV32/M33 | 1.506× | 1.100× | **1.039×** |

**BabyBear-Karatsuba is the most ISA-leveling configuration in the whole
project.** Phase 3.2.z ranged 1.21–1.51× depending on allocator. This run
drops to 1.04×, ten percent below the best allocator swap.

## Why Karatsuba helps Hazard3 but not M33

Hazard3 is a small in-order core with a minimal branch predictor. Every
`MUL` instruction costs a full cycle, and the register pressure of
Mont-reduction limits how much the pipeline can absorb. Cutting 7
Mont-muls per extension multiply (19 → 12) is a direct 7-cycle-per-call
saving.

Cortex-M33 has a pipelined UMULL + DSP extensions + a BTB-backed branch
predictor + LLVM's CSE-at-`opt-level=s-lto=fat` optimisation of the
schoolbook form. The 7 "saved" Mont-muls were either already folded by
the compiler or were latency-hidden by the pipeline. Net zero effect.

Hazard3 has neither the hardware nor the compiler scheduling that made
the compiler-level Karatsuba-equivalent "free" on M33. So it needed our
hand-written Karatsuba to see the win.

Rule extracted:
> Extension-arithmetic hand optimizations benefit in-order RISC cores
> (Hazard3, plausibly other RV32IMAC implementations) more than
> pipelined ARM cores at the same clock. This matters for picking
> where to spend optimization time: Hazard3 gets the easy wins.

## BabyBear still loses to Goldilocks

Even with Karatsuba, RV32 BabyBear-Quartic is 14.8 % slower than RV32
Goldilocks-Quadratic. Full picture:

- M33: BabyBear +66 % vs Goldilocks (unchanged by Karatsuba).
- RV32: BabyBear +15 % vs Goldilocks (down from +22 % pre-Karatsuba).

The 31-bit-field-fits-register hypothesis is not enough to overcome the
Quartic-vs-Quadratic extension overhead at 95-bit conjectured security.
See `research/postmortems/2026-04-24-babybear-quartic-regresses.typ`.

## Variance is the silver lining

`std_dev / mean = 0.053 %`. This is the tightest timing profile in the
whole project, better than the bump allocator's 0.076 %, better than
`TlsfHeap-Goldilocks` at 0.081 %. Likely cause: BabyBear's u32-native
arithmetic is branch-free on Hazard3's pipeline, and Karatsuba's
structured control flow has fewer mispredict opportunities than the
schoolbook's nested adds.

For side-channel-sensitive workloads on Hazard3, BabyBear-Karatsuba is
the most deterministic configuration available. Even if it's slower on
median, the timing *shape* is cleaner.

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
