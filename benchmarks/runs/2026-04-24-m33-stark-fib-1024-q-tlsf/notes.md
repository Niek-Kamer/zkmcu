# 2026-04-24 — Cortex-M33 STARK verify, Quadratic + TlsfHeap

Phase 3.2.z follow-on to the bump allocator experiment. BumpAlloc
gave silicon-baseline variance but required 384 KB arena (above the
128 KB production tier). This run tests `embedded-alloc::TlsfHeap` —
O(1) two-level segregated fit — as the middle ground: deterministic
alloc timing without giving up dealloc.

## Headline

**74.65 ms median**, **0.081 % std-dev variance**, 20 clean iterations.
**heap_peak = 93,515 B — identical to LlffHeap**, fits 128 KB tier.

This is the first configuration that's *both* silicon-baseline-variance
and 128-KB-tier-compliant.

## Variance matches BumpAlloc

| Measure | LlffHeap | BumpAlloc | *TlsfHeap* |
|---|---:|---:|---:|
| min-max / median | 0.245 % | 0.352 % | 0.343 % |
| IQR / median | 0.128 % | *0.082 %* | *0.113 %* |
| std-dev / mean | n/a | *0.080 %* | **0.081 %** |

Std-dev is the honest variance measure when comparing across
allocators. TLSF's 0.081 % is essentially identical to BumpAlloc's
0.080 % — both at silicon baseline.

IQR is slightly wider than bump (0.113 vs 0.082) because TLSF still
has per-op branches whose timing can vary with free-list state. Bump
wins IQR because every iteration starts byte-identical under its
watermark reset. TLSF gets close to that without the reset trick.

## The performance tax

| Allocator | Median | Δ vs LlffHeap |
|---|---:|---:|
| BumpAlloc | 67.95 ms | *-1.72 ms* (faster) |
| LlffHeap | 69.67 ms | baseline |
| TlsfHeap | 74.65 ms | **+4.99 ms** (slower) |

TLSF costs ~5 ms vs LlffHeap on M33. The per-alloc TLSF operation is
more work than LlffHeap's first-fit walk when the free list is short,
but bounded where LlffHeap's is not. 400 alloc sites × ~12.5 μs extra
each ≈ 5 ms. This is the price of O(1) worst-case bound.

For timing-deterministic use cases (hardware wallets, side-channel
resistance, real-time systems) this is usually worth paying. For
raw-throughput use cases, LlffHeap remains the choice.

## Memory is unchanged

`heap_peak` = 93,515 B, *byte-identical* to the LlffHeap number from
phase 3.2. Makes sense — TLSF tracks live allocations just like
LlffHeap, so peak live usage depends only on winterfell's allocation
pattern, not on the allocator's internal structure.

This confirms that **STARK verify at 95-bit conjectured security, with
a deterministic O(1) allocator, fits under 128 KB total RAM** (93.5 KB
heap + 5.6 KB stack + statics ≈ 100 KB, 28 KB margin).

## Reproduction

```bash
cargo build -p bench-rp2350-m33-stark --release
scp target/thumbv8m.main-none-eabihf/release/bench-rp2350-m33-stark \
    pid-admin@10.42.0.30:/tmp/m33tlsf.elf

picotool load -v -x -t elf /tmp/m33tlsf.elf
cat /dev/ttyACM0
```
