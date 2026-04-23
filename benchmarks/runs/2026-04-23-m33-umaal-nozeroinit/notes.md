# 2026-04-23 — M33 UMAAL asm, zero-init and preload micro-tuning

Follow-up to the session-3A run that introduced SRAM execution and register-resident operand rows. Two small changes layered on top:

1. **No zero-init of `t[0..15]`.** The previous body wrote 16 zero STRs up front. `t[8..15]` is always overwritten by a final-carry STR before any later UMAAL reads it, and `t[0..7]` is handled by a new `MUL_ROW_ZERO` macro that uses `movs r12, #0` before each UMAAL instead of loading zero from stack. Net saving: 16 STRs and 8 LDRs per `mul_reduce_armv8m` call, with 8 added MOVS — 16 memory operations removed.
2. **LDM for operand preloads.** The 8-individual-LDR sequence for the `b[]` preload (and again for `p[]`) becomes `ldm r1, {r4-r11}` / `ldm r2, {r4-r11}`. Smaller code, fewer bus transactions.

## Headline

**Groth16 verify: 641.6 ms (square), -1.2% vs 649.2 ms from session 3A.** 35% below the 988 ms crates.io baseline.

## Deltas

| Bench                      | Session 3A  | This run      | Δ         |
|----------------------------|-------------|---------------|----------:|
| groth16_verify (square)    | 649.2 ms    | **641.6 ms**  | -1.2%     |
| groth16_verify (sq5)       | 655.5 ms    | 647.8 ms      | -1.2%     |
| groth16_verify (semaphore) | 770.0 ms    | 760.7 ms      | -1.2%     |
| pairing (single)           | 353.9 ms    | 350.0 ms      | -1.1%     |
| g1_mul (typical)           | 33.6 ms     | 32.9 ms       | -2.1%     |
| g2_mul (typical)           | 153.2 ms    | 149.7 ms      | -2.3%     |

## Footprint

| Metric                     | Session 3A  | This run  | Δ     |
|----------------------------|-------------|-----------|------:|
| `.text` (bytes)            | 73,632      | 73,524    | -108  |
| `mul_reduce_armv8m` (size) | 2,146       | 2,038     | -108  |

Instruction mix inside `mul_reduce_armv8m`:

| Class              | 3A   | this | Δ   |
|--------------------|-----:|-----:|----:|
| LDR (16+32-bit)    | 204  | 180  | -24 |
| STR (16+32-bit)    | 196  | 180  | -16 |
| UMAAL              | 128  | 128  | 0   |
| ADCS               | 28   | 28   | 0   |
| MOVS               | 17   | 24   | +7  |
| LDM                | 0    | 2    | +2  |

STR count drop (-16) directly matches the removed zero-init. LDR drop (-24) is (-16 from two LDM substitutions) + (-8 from row 0 using MOVS). MOVS count bumps (+7) are net of the 8 new row-0 seeds and the 1 zero-init seed we deleted.

## Cumulative progress

| Layer                          | Square verify | Δ step    | Δ cumulative |
|--------------------------------|---------------|-----------|-------------:|
| crates.io 0.6.0                | 988 ms        | —         | 0%           |
| Fork of paritytech/bn master   | 962 ms        | -2.6%     | -2.6%        |
| + UMAAL `mul_reduce` asm (flash)| 697 ms       | -27.5%    | -29.5%       |
| + SRAM + register-live operands | 649 ms       | -6.9%     | -34.3%       |
| + no-zero-init + LDM preloads   | **641.6 ms** | **-1.2%** | **-35.0%**   |

## Variance note

15 complete iterations for the full-verify benches, 16 for g1/g2_mul. Square verify variance is ±0.08 ms (min 641.52, max 641.72). Tight enough that the -1.2% shift is definitely real, not noise.

## Reproduction

Changes to `vendor/bn/src/arith.rs` only:
- New `MUL_ROW_ZERO` macro that seeds UMAAL RdLo with MOVS rather than LDR.
- Function body replaces the 16-STR zero-init block with nothing, and the eight individual LDR preloads with a single LDM per phase.
- `MUL_ROW_REG 0` replaced with `MUL_ROW_ZERO` in the Phase 1 sequence.

Firmware crate and memory.x unchanged from session 3A.
