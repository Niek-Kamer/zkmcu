---
title: Benchmarks
description: Directly-measured performance numbers from the Raspberry Pi Pico 2 W. Both curves, real-world circuit.
---

All numbers measured on-device via USB-CDC serial output. No emulation, no extrapolation. Full per-run data (raw serial logs + structured TOML + observations) lives under [`benchmarks/runs/`](https://github.com/Niek-Kamer/zkmcu/tree/main/benchmarks/runs) in the repo.

## Headline

Raspberry Pi Pico 2 W @ 150 MHz, both curves, both cores:

| verify | Cortex-M33 | Hazard3 RV32 | RV32 / M33 |
|---|---:|---:|---:|
| Groth16 / BN254, 1 public input (`square`) | **962 ms** | 1,341 ms | 1.39× |
| **Groth16 / BN254, real Semaphore depth-10 (4 public inputs)** | **1,176 ms** | 1,564 ms | 1.33× |
| Groth16 / BLS12-381, 1 public input (`square`) | 2,015 ms | 5,151 ms | 2.56× |

Iteration-to-iteration variance stays under **0.07 %** on all rows, **0.030 %** on the Semaphore rows. Every iteration returns `ok=true`. No flakiness.

The Semaphore row is the one to pay attention to. It's a real VK + proof from the production Semaphore v4.14.2 trusted setup, not a synthetic circuit we generated ourselves. See [Semaphore](/semaphore/) for the full setup.

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

First public `no_std` BLS12-381 Groth16 verifier on Cortex-M that I could find. Full prediction-vs-measurement comparison in [`research/reports/2026-04-22-bls12-381-results.typ`](https://github.com/Niek-Kamer/zkmcu/blob/main/research/reports/2026-04-22-bls12-381-results.typ).

## Memory

Directly measured on-device via stack painting + a tracking-heap allocator wrapper. Both curves, both cores:

| | BN254 M33 | BN254 RV32 | BLS12 M33 | BLS12 RV32 |
|-|---:|---:|---:|---:|
| Peak stack during verify | 15.6 KB | 15.7 KB | 19.4 KB | 20.6 KB |
| Peak heap during verify | 81.3 KB | (pending) | **79.4 KB** | (pending) |
| Heap arena (configured) | **96 KB** | 256 KB | 256 KB | 256 KB |
| Total RAM used | **≈ 111 KB** | ≈ 272 KB | ≈ 116 KB | ≈ 277 KB |

The interesting result: BLS12-381 on zkcrypto uses *less* heap than BN254 on substrate-bn (79 KB vs 81 KB), because zkcrypto keeps Miller-loop line coefficients in stack-allocated `G2Prepared` where substrate-bn heap-allocates an Fq12 polynomial workspace. Total RAM shifts slightly more to stack, but the aggregate is within 5 KB across both curves on M33.

Both curves fit comfortably on any 128 KB SRAM-class MCU: nRF52832, STM32F405, Ledger ST33K1M5, Infineon SLE78. See [architecture](/architecture/) for why the heap dominates and [security](/security/) for the DoS-hardening on the parsers.

## Public-input scaling (BN254)

The `vk_x = IC[0] + Σ x[i] · IC[i+1]` step is a G1 scalar multiplication per public input. Cost depends on the numerical size of the scalar, not just the count:

| Input shape | Scalar bits | Extra cost per input, M33 |
|---|---:|---:|
| Counter / index | < 2^16 | ~3 ms |
| Ethereum address | ~160 | ~40 ms |
| Merkle root / hash output | ~254 random | **~71 ms** |

Semaphore's 4 public inputs (merkle root, nullifier, hash-of-message, hash-of-scope) are all full 254-bit scalars — they land in the bottom row. A 10-public-input circuit with merkle-root-shaped inputs takes ~1.6 s; the same circuit shape with counter-shaped inputs takes ~990 ms. Circuit designers targeting embedded verify should fold public state into a single hash-commitment `Fr` if at all possible. **Per-input cost differs by 24×** between the two regimes.

## Cross-ISA comparison

Same source, same silicon, different ISA. Cortex-M33 wins the overall verify on both curves: by ~28 % on BN254 (962 vs 1,341 ms), and by ~60 % on BLS12-381 (2,015 vs 5,151 ms).

On the simpler primitive — G1 scalar multiplication — BN254 lands Hazard3 within 5 % of Cortex-M33 (62 vs 65 ms). An earlier baseline (pre dep-bump of `substrate-bn`) showed Hazard3 ~35 % *faster* on G1 mul; that gap disappeared once upstream substrate-bn rolled an optimisation pass that disproportionately helped ARM codegen. Worth flagging as evidence that cross-ISA comparisons on pure-Rust crypto are sensitive to compiler + library versions; reproducing requires pinning both.

**On BLS12-381 the cross-ISA story is different.** Hazard3 *loses* on every primitive: 69 % slower on G1 scalar mul, 92 % slower on G2 mul, 226 % slower on pairing. Full writeup in [`research/reports/2026-04-22-bls12-381-results.typ`](https://github.com/Niek-Kamer/zkmcu/blob/main/research/reports/2026-04-22-bls12-381-results.typ). Short version: Cortex-M33's `UMAAL` multiply-accumulate instruction wins big on BLS12's 12-word Fp where it didn't matter much at BN254's 8-word size. Cross-ISA conclusions on pairing-friendly curves are prime-size dependent, not algorithm dependent.

No prior public cross-ISA benchmark of pairing-grade arithmetic on identical silicon exists — so whichever direction the comparison lands, the numbers are new data.

## Reproducing the numbers

The four firmware crates (`bench-rp2350-{m33,rv32}{,-bls12}`) run their respective benchmark suite over USB-CDC serial. A Raspberry Pi Pico 2 W, a dev host to flash from, and `picotool` is everything you need. Flash and connect instructions are in the [repo README](https://github.com/Niek-Kamer/zkmcu#flashing).

Each run writes:

- `raw.log` — verbatim serial capture
- `result.toml` — structured, schema-versioned
- `notes.md` — observations, anomalies, and what was deliberately not measured
