---
title: Benchmarks
description: Directly-measured performance numbers from the Raspberry Pi Pico 2 W.
---

All numbers measured on-device via USB-CDC serial output. No emulation, no extrapolation. Full per-run data (raw serial logs + structured TOML + observations) lives under [`benchmarks/runs/`](https://github.com/Niek-Kamer/zkmcu/tree/main/benchmarks/runs) in the repo.

## Headline

| Operation | Cortex-M33 | Hazard3 RV32 |
|---|---:|---:|
| G1 scalar mul (typical) | 62 ms | 65 ms |
| G2 scalar mul (typical) | 207 ms | 283 ms |
| BN254 pairing | 535 ms | 707 ms |
| **Groth16 verify** (1 public input) | **962 ms** | **1,341 ms** |

At 150 MHz system clock. Iteration-to-iteration variance < 0.07 % on both cores. Verify numbers are from the shipping 96 KB heap-arena configuration ([`2026-04-22-m33-heap-96k-confirmed`](https://github.com/Niek-Kamer/zkmcu/tree/main/benchmarks/runs/2026-04-22-m33-heap-96k-confirmed) and [`2026-04-21-rv32-stack-painted`](https://github.com/Niek-Kamer/zkmcu/tree/main/benchmarks/runs/2026-04-21-rv32-stack-painted)); per-operation breakdowns come from the stack-painted runs of the same firmware.

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

## Cross-ISA comparison

Same source, same silicon, different ISA. Cortex-M33 wins the overall verify by ~28 % (962 ms vs 1,341 ms), driven by the higher-order `Fp²` / `Fp¹²` arithmetic that dominates the pairing. On the simpler primitive — G1 scalar multiplication — the two ISAs land within 5 % of each other (62 ms vs 65 ms), and on small-scalar best-case G1 they're functionally tied (32 ms vs 34 ms).

An earlier baseline (pre dep-bump of `substrate-bn`) showed Hazard3 ~35 % *faster* on G1 mul; that gap disappeared once upstream substrate-bn rolled an optimisation pass that disproportionately helped ARM codegen. Worth flagging as evidence that cross-ISA comparisons on pure-Rust crypto are sensitive to compiler + library versions, and that reproducing requires pinning both.

No prior public cross-ISA benchmark of pairing-grade arithmetic on identical silicon exists — so whichever direction the comparison lands, the numbers themselves are new data.

## Reproducing the numbers

The firmware crates (`bench-rp2350-m33`, `bench-rp2350-rv32`) run the benchmark suite over USB-CDC serial. A Raspberry Pi Pico 2 W, a dev host to flash from, and `picotool` is everything you need. Flash and connect instructions are in the [repo README](https://github.com/Niek-Kamer/zkmcu#flashing).

Each run writes:

- `raw.log` — verbatim serial capture
- `result.toml` — structured, schema-versioned
- `notes.md` — observations, anomalies, and what was deliberately not measured
