---
title: Benchmarks
description: Directly-measured performance numbers from the Raspberry Pi Pico 2 W. Three verifier families, two ISAs, three allocator strategies.
---

All numbers measured on-device via USB-CDC serial output. No emulation, no extrapolation. Full per-run data (raw serial logs + structured TOML + observations) lives under [`benchmarks/runs/`](https://github.com/Niek-Kamer/zkmcu/tree/main/benchmarks/runs) in the repo.

## Headline, all verifier families

Raspberry Pi Pico 2 W @ 150 MHz, production allocator configs (LlffHeap for Groth16, TlsfHeap for STARK), cross-ISA comparison:

| Verify | Cortex-M33 | Hazard3 RV32 | RV32 / M33 | Proof size |
|---|---:|---:|---:|---:|
| **STARK Fibonacci-1024**, 95-bit conjectured | **75 ms** | 112 ms | 1.51× | 30.9 KB |
| Groth16 / BN254, 1 public input (`square`) | 962 ms | 1,341 ms | 1.39× | 256 B |
| **Groth16 / BN254, real Semaphore depth-10** (4 public inputs) | **1,176 ms** | 1,564 ms | 1.33× | 256 B |
| Groth16 / BLS12-381, 1 public input (`square`) | 2,015 ms | 5,151 ms | 2.56× | 512 B |

STARK verify is **15-27× faster than Groth16** on the same silicon. The tradeoff is proof size: 30.9 KB vs 256-512 B. Classic throughput-for-bandwidth swap. Pick Groth16 when the transport is bandwidth-bound (LoRa, NFC); pick STARK when verify latency matters (per-packet, hot loops).

The Semaphore row is the one to pay attention to for production adoption. It's a real VK + proof from the Semaphore v4.14.2 trusted setup, not a synthetic circuit we generated ourselves. See [Semaphore](/semaphore/) for the full setup.

## STARK verify, allocator matrix

Fibonacci-1024 AIR, `FieldExtension::Quadratic` (95-bit conjectured security). Three allocator strategies, two ISAs:

| Allocator | M33 median | M33 std-dev | M33 heap peak | RV32 median | RV32 std-dev | Fits 128 KB? |
|---|---:|---:|---:|---:|---:|:---:|
| LlffHeap (linked-list first-fit) | 69.7 ms | 0.13 % IQR | 93.5 KB | 92.4 ms | ~0.46 % | ✓ |
| **TlsfHeap (O(1), production default)** | **74.7 ms** | **0.08 %** | **93.5 KB** | **112.4 ms** | **0.11 %** | **✓** |
| BumpAlloc (watermark-reset, benchmark only) | 67.9 ms | 0.08 % | 314 KB | 82.2 ms | 0.08 % | ✗ |

**TlsfHeap is the production pick**, it gets you the variance floor of the bump allocator while keeping heap peak at LlffHeap's 93.5 KB and fitting the 128 KB tier. The 5 ms cost on M33 (20 ms on RV32) is the price of O(1) worst-case bound. For hardware wallets where verify runs at human action speed, that's indistinguishable; for hot loops, LlffHeap still wins on raw throughput.

Full story on where this data comes from and what it means for side-channel resistance: see [Deterministic timing](/determinism/).

## STARK per-component, Cortex-M33

Rough cost decomposition of the 75 ms verify (TlsfHeap, Quadratic):

| Component | Cycles (≈) | ms (≈) | Share |
|---|---:|---:|---:|
| Blake3 compressions (~500-700) | ~5.5M | ~37 | 50 % |
| Goldilocks $F_(p^2)$ mul / fold | ~2.8M | ~19 | 25 % |
| Merkle auth-path + parse + scratch | ~2.9M | ~19 | 25 % |

Hash work is the dominant cost. Blake3 falls back to pure-Rust on both embedded targets (no SIMD available on the M33 or Hazard3), which keeps numbers reproducible but leaves ~2× headroom if someone writes a hand-tuned Thumb-2 blake3 inner loop.

## Per-op breakdown, BN254

| Operation | Cortex-M33 | Hazard3 RV32 |
|---|---:|---:|
| G1 scalar mul (typical) | 62 ms | 65 ms |
| G2 scalar mul (typical) | 207 ms | 283 ms |
| BN254 pairing | 535 ms | 707 ms |
| **Groth16 verify** (1 public input) | **962 ms** | **1,341 ms** |

Per-op numbers from the stack-painted runs of the same firmware. Verify numbers from the shipping 96 KB heap-arena configuration ([`2026-04-22-m33-heap-96k-confirmed`](https://github.com/Niek-Kamer/zkmcu/tree/main/benchmarks/runs/2026-04-22-m33-heap-96k-confirmed) and [`2026-04-21-rv32-stack-painted`](https://github.com/Niek-Kamer/zkmcu/tree/main/benchmarks/runs/2026-04-21-rv32-stack-painted)).

## Per-op breakdown, BLS12-381

| Operation | Cortex-M33 | Hazard3 RV32 |
|---|---:|---:|
| G1 scalar mul | 847 ms | 1,427 ms |
| G2 scalar mul | 523 ms | 1,003 ms |
| pairing | 607 ms | 1,975 ms |
| **Groth16 verify** (1 public input) | **2,015 ms** | **5,151 ms** |

First public `no_std` BLS12-381 Groth16 verifier on Cortex-M that we could find. Full prediction-vs-measurement comparison in [`research/reports/2026-04-22-bls12-381-results.typ`](https://github.com/Niek-Kamer/zkmcu/blob/main/research/reports/2026-04-22-bls12-381-results.typ).

## Memory

Directly measured on-device via stack painting + a tracking-heap allocator wrapper. All three verifier families, Cortex-M33:

| | BN254 Groth16 | BLS12 Groth16 | STARK (TlsfHeap) |
|-|---:|---:|---:|
| Peak stack during verify | 15.6 KB | 19.4 KB | 5.6 KB |
| Peak heap during verify | 81.3 KB | 79.4 KB | 93.5 KB |
| Heap arena configured | 96 KB | 256 KB | 256 KB |
| **Total RAM** | ≈ 97 KB | ≈ 99 KB | **≈ 100 KB** |

All three fit comfortably on any 128 KB SRAM-class MCU: nRF52832, STM32F405, Ledger ST33K1M5, Infineon SLE78. That's the phase-3 finding: **zkmcu is the first open `no_std` family of SNARK and STARK verifiers that all fit the hardware-wallet-tier SRAM budget at production-grade security**.

STARK verify surprises on the stack side, only 5.6 KB, vs 15-20 KB for Groth16. Winterfell routes most verify state through the heap allocator rather than stack frames, and the cost of that is *not* a bigger stack.

## Public-input scaling (BN254)

The `vk_x = IC[0] + Σ x[i] · IC[i+1]` step is a G1 scalar multiplication per public input. Cost depends on the numerical size of the scalar, not just the count:

| Input shape | Scalar bits | Extra cost per input, M33 |
|---|---:|---:|
| Counter / index | < 2^16 | ~3 ms |
| Ethereum address | ~160 | ~40 ms |
| Merkle root / hash output | ~254 random | **~71 ms** |

Semaphore's 4 public inputs (merkle root, nullifier, hash-of-message, hash-of-scope) are all full 254-bit scalars, they land in the bottom row. A 10-public-input circuit with merkle-root-shaped inputs takes ~1.6 s; the same circuit shape with counter-shaped inputs takes ~990 ms. Circuit designers targeting embedded verify should fold public state into a single hash-commitment `Fr` if at all possible. **Per-input cost differs by 24×** between the two regimes.

## Cross-ISA, three families

Same source, same silicon, different ISA. Cortex-M33 wins the overall verify on every proof system. But the ratio swings a lot:

| Verifier family | RV32 / M33 | What's driving the gap |
|---|---:|---|
| STARK Fibonacci-1024 | **1.51×** (TlsfHeap) | TLSF bitmap walks mispredict more on Hazard3 |
| STARK Fibonacci-1024 | 1.21× (BumpAlloc) | **allocator-free cross-ISA ratio: pure crypto** |
| BN254 Groth16 | 1.33× | G2 scalar mul + pairing tower |
| BLS12-381 Groth16 | 2.56× | UMAAL wins at 12-word Fp where it didn't at 8 |

The STARK rows are the big new finding. With `BumpAlloc` (allocator overhead stripped out) the cross-ISA ratio is 1.21×, the honest "pure Blake3 + Goldilocks $F_(p^2)$ arithmetic" number. With `TlsfHeap` it widens to 1.51× because Hazard3 pays more per mispredicted branch in TLSF's bitmap walks. And with `LlffHeap` it lands between at 1.33×. **An allocator choice can swing the M33-vs-Hazard3 conclusion by 30 %.** Any cross-ISA crypto benchmark using a stock general-purpose allocator is partially measuring the allocator, not the workload. See [Deterministic timing](/determinism/) for the full trace.

**BLS12-381 cross-ISA:** Hazard3 *loses* on every primitive at 12-word Fp. Full writeup in [`research/reports/2026-04-22-bls12-381-results.typ`](https://github.com/Niek-Kamer/zkmcu/blob/main/research/reports/2026-04-22-bls12-381-results.typ). Short version: Cortex-M33's `UMAAL` multiply-accumulate instruction wins big on BLS12's 12-word Fp where it didn't matter much at BN254's 8-word size. Cross-ISA conclusions on pairing-friendly curves are prime-size dependent, not algorithm dependent.

## Reproducing the numbers

The six firmware crates (`bench-rp2350-{m33,rv32}{,-bls12,-stark}`) run their respective benchmark suite over USB-CDC serial. A Raspberry Pi Pico 2 W, a dev host to flash from, and `picotool` is everything you need.

Each run writes:

- `raw.log`, verbatim serial capture
- `result.toml`, structured, schema-versioned
- `notes.md`, observations, anomalies, and what was deliberately not measured

```bash
# Dev machine:
cargo build -p bench-rp2350-m33-stark --release
scp target/thumbv8m.main-none-eabihf/release/bench-rp2350-m33-stark \
    <pi-host>:/tmp/bench.elf

# Pi 5 with Pico in BOOTSEL:
picotool load -v -x -t elf /tmp/bench.elf
cat /dev/ttyACM0
```

Flash the other three curve/ISA combinations by swapping the crate name. Full list: [`crates/bench-*`](https://github.com/Niek-Kamer/zkmcu/tree/main/crates).
