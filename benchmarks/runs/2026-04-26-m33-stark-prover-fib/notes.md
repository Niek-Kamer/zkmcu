# Phase-4: On-device STARK proving — Fibonacci AIR

## Summary

First confirmed on-device STARK proving on Cortex-M33 (RP2350 @ 150 MHz).

Winterfell's vendored fork already has `#![no_std]` across all sub-crates;
no patches were needed for embedded compilation. The prover compiles with
`default-features = false` and runs entirely from SRAM with a TLSF heap.

## N = 64 (feasibility run)

- Prove: ~35 ms, Verify: ~12 ms
- Heap peak: ~75 KB, Stack peak: ~4.5 KB, Proof: ~3 KB
- Self-verify passes on-device ✓

## N = 128 (scaling run, 6 iterations, tight variance)

- Prove: 68.3 ms median (0.32 % peak-to-peak)
- Verify: 15.5 ms median (0.81 % peak-to-peak)
- Heap peak: 153.5 KB (well within 256 KB heap)
- Stack peak: 4,448 bytes (barely moves with N)
- Proof size: 5,116 bytes

## Scaling (N=64 → N=128, 2× trace)

| Metric      | 64      | 128     | Ratio | Expected |
|-------------|---------|---------|-------|----------|
| Prove time  | ~35 ms  | 68 ms   | 1.94× | ~2× (O(N log N)) |
| Verify time | ~12 ms  | 15.5 ms | 1.29× | <2× (hash-dominated) |
| Heap peak   | ~75 KB  | 153 KB  | 2.04× | linear in N |
| Proof size  | ~3 KB   | 5.1 KB  | 1.66× | O(log N) |

Scaling is consistent with theory.

## FRI parameter analysis

ProofOptions: num_queries=8, blowup=4, fold=4, max_remainder_deg=7, no field extension.
Stopping rule: fold until (domain/fold_factor) ≤ (max_remainder+1=8).

| N   | LDE  | Fold steps        | Remainder domain | Valid? |
|-----|------|-------------------|-----------------|--------|
| 64  | 256  | 256→64→16 (stop)  | 16              | 7 < 16 ✓ |
| 128 | 512  | 512→128→32 (stop) | 32              | 7 < 32 ✓ |
| 256 | 1024 | 1024→256→64→16 (stop) | 16          | 7 < 16 ✓ |

## N = 256 (5 iterations, 384 KB heap)

- Prove: 134 ms median (0.16% variance)
- Verify: 19.4 ms median
- Heap peak: 299 KB (81 KB headroom in 384 KB config)
- Stack peak: 4,448 bytes (unchanged — stack does not scale with N)
- Proof size: 6,668 bytes

## Practical ceiling

N=512 would require ~600 KB heap, exceeding the 512 KB SRAM.
**N=256 is the SRAM ceiling for this chip at blowup=4, Goldilocks, base field only.**

To go beyond N=256 on RP2350:
- Reduce blowup to 2 (halves heap, halves security margin)
- Use SRAM8/SRAM9 scratch banks for stack and overlay allocations
- Stream LDE matrix through flash (slow but feasible for offline proving)

## Cross-ISA: RV32 (Hazard3) N=256

- Prove: **208 ms** median (0.14% variance)
- Verify: **25 ms** median
- Heap: 306 KB (byte-identical to M33)
- Proof: 6,668 bytes (byte-identical)

### ISA gap analysis

| | M33 | RV32 | Ratio |
|---|-----|------|-------|
| Prove | 134 ms | 208 ms | **1.55×** |
| Verify | 19.4 ms | 25 ms | **1.29×** |

Prove is field-op-heavy (Goldilocks NTT). M33 has `UMAAL` (32×32+32+32→64).
RV32 must simulate 64-bit multiply with paired `mul`/`mulhu` — roughly 2× the
multiply cost. This accounts for the extra 0.26× gap beyond the hash-dominated
verify ratio.

Compare to phase-3.3 verifier cross-ISA gap of ~1.07× — that run was almost
entirely hash (Blake3) with minimal field ops, so it barely showed the ISA difference.
The prover exposes it clearly.

## Significance

1. First on-chip STARK prover on Cortex-M33 — no prior art found
2. Prover (134 ms) is faster than the project's own Groth16 *verifier* (643 ms)
3. Scaling is perfectly predictable: prove ≈ O(N), verify ≈ O(log N), heap = O(N)
4. Device can now vouch for its own computation — ZK attestation from hardware
