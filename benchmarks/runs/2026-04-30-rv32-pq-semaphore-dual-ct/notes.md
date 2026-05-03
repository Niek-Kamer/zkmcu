# Phase H — RV32 dual-hash CT honest path (2026-04-30)

## What this captures

26 iterations of `verify_constant_time(DUAL_PROOF_P2, DUAL_PROOF_B3, DUAL_PUBLIC)` on the unmodified `pq-semaphore-d10-dual` triple, run on the Pico 2 W's Hazard3 RV32 core @ 150 MHz, under the firmware crate `bench-rp2350-rv32-pq-semaphore-dual-ct`.

Honest-path only — see the M33 sibling run (`benchmarks/runs/2026-04-30-m33-pq-semaphore-dual-ct/notes.md`) for the full Phase H scope and the open issue blocking the on-silicon reject-path capture on both ISAs.

## Result vs prediction

Phase F RV32 honest: 2041.0 ms (`benchmarks/runs/2026-04-30-rv32-pq-semaphore-dual-q17/`).

CT honest measured: 2049.312 ms median (n=26).

Delta: +8.3 ms, **+0.41 %** above Phase F. Hardening plan budgeted +1 %. Inside budget.

`range_pct` = 0.053 %, inside the < 0.1 % per-pattern target.

`heap_peak` = 304_180 B — drop-between pattern preserved across the CT entry point on RV32 as well.

## Side-by-side

| ISA | Phase F honest | Phase H CT honest | Δ      |
|-----|---------------:|------------------:|-------:|
| M33 | 1611.44 ms     | 1620.64 ms        | +0.57 %|
| RV32| 2041.0 ms      | 2049.31 ms        | +0.41 %|

CT path costs roughly +9 ms / +0.5 % on either ISA — purely the bookkeeping in `verify_constant_time` (boolean composition, fallback parse attempts on the public input, double-Result tracking). Verify itself dominates and is identical to Phase F.
