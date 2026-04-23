# 2026-04-24 — Hazard3 RV32 STARK verify, Quadratic + TlsfHeap

Companion to `2026-04-24-m33-stark-fib-1024-q-tlsf`. Same AIR, same
proof bytes, same firmware shape aside from ISA. The pair measures
whether TLSF's "deterministic O(1) alloc, keep 128 KB tier" story
survives the jump to RISC-V.

## Headline

**112.40 ms median**, **0.110 % std-dev variance**, 23 clean
iterations. Tight compared to LlffHeap's 0.456 %, but ~30 ms slower
than bump and 20 ms slower than LlffHeap.

## TLSF costs RV32 more than M33

The central finding of this run: *TLSF is proportionally more
expensive on Hazard3 than on Cortex-M33*, in a way that bump alloc
wasn't.

| ISA | LlffHeap | BumpAlloc | TlsfHeap | TLSF vs LlffHeap |
|---|---:|---:|---:|---:|
| Cortex-M33 | 69.67 ms | 67.95 ms | 74.65 ms | **+5 ms** |
| Hazard3 RV32 | 92.39 ms | 82.17 ms | 112.40 ms | **+20 ms** |

Cross-ISA ratio:

| Config | Ratio | Direction |
|---|---:|---|
| LlffHeap | 1.33× | baseline |
| BumpAlloc | 1.21× | narrower — allocator neutralised |
| *TlsfHeap* | *1.51×* | *widest — TLSF amplifies Hazard3 overhead* |

Best guess on why: TLSF's per-alloc op walks a two-level bitmap
(first-level index, then second-level index) with conditional
branches at each step. Each branch can mispredict. Hazard3's
in-order pipeline + minimal branch predictor pays a full pipeline
flush per mispredict; Cortex-M33's BTB-backed predictor absorbs
some of them. Over ~400 alloc sites × multiple branches each, the
cost compounds differently between the two cores.

## Variance still wins vs LlffHeap

Even at the performance cost, TLSF is substantially more deterministic
than LlffHeap on RV32:

| Measure | LlffHeap | *TlsfHeap* | BumpAlloc |
|---|---:|---:|---:|
| min-max / median | 0.46 % | *0.41 %* | 0.33 % |
| std-dev / mean | n/a | *0.110 %* | 0.076 % |

TLSF halves the min-max spread we saw under clone-hoisted LlffHeap.
std-dev lands at 0.11 %, within the "deterministic for side-channel
purposes" band (anything under ~0.2 % is sub-mispredict-noise).

## Production implication

For RV32 specifically, the tradeoff shifts:

- LlffHeap: fastest (92 ms) but sloppiest variance (~0.46 %)
- TlsfHeap: slowest (112 ms), tighter variance (0.11 %)
- BumpAlloc: fastest + tightest (82 ms, 0.08 %) but 253 KB arena

If the device is 128-KB-tier-constrained *and* needs variance-tight
timing, the TLSF penalty on Hazard3 is 20 ms (22 %) over LlffHeap.
For hardware-wallet workloads where verify runs at human-speed (once
per confirmation), that's usually fine. For anything in a hot
path, pick bump + more SRAM, or LlffHeap + accept variance.

## Reproduction

```bash
cargo build -p bench-rp2350-rv32-stark --release
scp target/riscv32imac-unknown-none-elf/release/bench-rp2350-rv32-stark \
    pid-admin@10.42.0.30:/tmp/rv32tlsf.elf

picotool load -v -x -t elf /tmp/rv32tlsf.elf
cat /dev/ttyACM0
```
