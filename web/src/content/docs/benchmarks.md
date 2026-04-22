---
title: Benchmarks
description: Directly-measured performance numbers from the Raspberry Pi Pico 2 W.
---

All numbers measured on-device via USB-CDC serial output. No emulation, no extrapolation. Full per-run data (raw serial logs + structured TOML + observations) lives under [`benchmarks/runs/`](https://github.com/Niek-Kamer/zkmcu/tree/main/benchmarks/runs) in the repo.

## Headline

| Operation | Cortex-M33 | Hazard3 RV32 |
|---|---:|---:|
| G1 scalar mul (typical) | 110 ms | 72 ms |
| G2 scalar mul (typical) | 210 ms | 284 ms |
| BN254 pairing | 533 ms | 701 ms |
| **Groth16 verify** (1 public input) | **962 ms** | **1,341 ms** |

At 150 MHz system clock. Iteration-to-iteration variance < 0.07 % on both cores.

## Memory

| Region | Cortex-M33 |
|---|---:|
| `.text` | 73 KB |
| Peak stack during verify | 15.6 KB |
| Peak heap during verify | 79.4 KB |
| Heap arena (confirmed sufficient) | 96 KB |
| **Total RAM used** | **~112 KB** |

Fits comfortably on any 128 KB SRAM-class MCU: nRF52832, STM32F405, Ledger ST33K1M5, Infineon SLE78. See [architecture](/architecture/) for why the heap dominates and [security](/security/) for the DoS-hardening added in v0.1.0.

## Public-input scaling

The `vk_x = IC[0] + Σ x[i] · IC[i+1]` step is a G1 scalar multiplication per public input. Cost depends on the numerical size of the scalar, not just the count:

| Input shape | Scalar bits | Extra cost per input |
|---|---:|---:|
| Counter / index | < 2^16 | ~3 ms |
| Ethereum address | ~160 | ~40 ms |
| Merkle root / hash output | 256 random | ~67 ms |

A 10-public-input circuit with Merkle-root-shaped inputs takes ~1.6 s. The same circuit shape with counter-shaped inputs takes ~990 ms. This matters when designing circuits: packing small values into one Fr vs. leaving them separate affects verify latency by up to 20×.

## The RISC-V surprise

Hazard3 runs G1 scalar multiplication **35 % faster** than Cortex-M33 on identical silicon. `substrate-bn` is pure Rust and doesn't use ARM's `SMLAL`/`UMAAL` multiply-accumulate intrinsics, so the expected ARM advantage isn't realised. Hazard3's 31 general-purpose registers (vs. Thumb-2's 13) reduce register spills during schoolbook Fp multiplication, which dominates G1 cost.

On higher-order operations (Fp², Fp¹², the full pairing) other factors flip the result and Cortex-M33 wins the overall verify by 34 %. But the G1 result is genuinely new — no prior public cross-ISA benchmark of pairing-grade arithmetic on identical silicon has called it out.

## Reproducing the numbers

The firmware crates (`bench-rp2350-m33`, `bench-rp2350-rv32`) run the benchmark suite over USB-CDC serial. A Raspberry Pi Pico 2 W, a dev host to flash from, and `picotool` is everything you need. Flash and connect instructions are in the [repo README](https://github.com/Niek-Kamer/zkmcu#flashing).

Each run writes:

- `raw.log` — verbatim serial capture
- `result.toml` — structured, schema-versioned
- `notes.md` — observations, anomalies, and what was deliberately not measured
