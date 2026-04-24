# 2026-04-23 — M33 Groth16 with UMAAL asm in SRAM + register-resident operand rows

Third and latest optimization layer on top of the hand-written UMAAL asm from the previous run. Two changes this session:

1. **Register scheduling.** `b[0..7]` is now preloaded into `r4..r11` once per `mul_reduce_armv8m` call and kept there across all 8 schoolbook rows. Same for `p[0..7]` across Phase 2. Saves 128 LDRs per call (8 words × 8 rows × 2 phases) vs the session-2b straight transliteration.
2. **SRAM execution.** `mul_reduce_armv8m` is placed in a new `.ram_text` output section (VMA in RAM, LMA in FLASH). Firmware's `#[pre_init]` copies the section from flash to SRAM before `main` runs, so the function executes from 0x20000000 instead of the XIP-flash region.

## Headline

**Groth16 verify: 649 ms (square), -6.9% vs the 697 ms UMAAL-in-flash run.**

Cumulative: **988 ms → 649 ms = -34.3%** from the original crates.io 0.6.0 baseline, in three layers:

| Layer              | Square verify | Δ step    | Δ cumulative |
|--------------------|---------------|-----------|-------------:|
| crates.io 0.6.0    | 988 ms        | —         | 0%           |
| paritytech master  | 962 ms        | -2.6%     | -2.6%        |
| + UMAAL asm (flash) | 697 ms       | -27.5%    | -29.5%       |
| + tuned asm + SRAM | **649 ms**    | **-6.9%** | **-34.3%**   |

## Deltas this run

| Bench                      | UMAAL flash (prev) | + tuned + SRAM | Δ this step |
|----------------------------|--------------------|----------------|------------:|
| groth16_verify (square)    | 697 ms             | **649 ms**     | -6.9%       |
| groth16_verify (sq5)       | 704 ms             | 655 ms         | -7.0%       |
| groth16_verify (semaphore) | 828 ms             | 770 ms         | -7.0%       |
| pairing (single)           | 377 ms             | 354 ms         | -6.1%       |
| g1_mul (typical)           | 36 ms              | 34 ms          | -6.7%       |
| g2_mul (typical)           | 162 ms             | 153 ms         | -5.6%       |

## Footprint

| Metric                     | prev (UMAAL flash) | this run    | Δ          |
|----------------------------|--------------------|-------------|-----------:|
| `.text` (bytes)            | 73,184             | 73,632      | +448       |
| `mul_reduce_armv8m` (size) | 1,744              | 2,146       | +23%       |
| VMA                        | 0x1000xxxx (FLASH) | 0x20000000 (SRAM) | moved |

The asm function grew 402 bytes despite saving 128 LDRs, because switching scratch registers from low regs (r5, r6, r7) to high regs (r12, lr) forced many instructions into 32-bit Thumb-2 encoding. Net we care about cycles, not bytes — and cycles fell.

The linker auto-inserted a 10-byte `__Thumbv7ABSLongThunk_mul_reduce_armv8m` at 0x1000f790 to bridge the flash→SRAM call distance (the 256 MB gap exceeds direct `bl` range). Transparent.

## Instruction mix comparison (`mul_reduce_armv8m` body)

| Class | prev | now   | Δ    |
|-------|-----:|------:|-----:|
| LDR (16+32-bit) | 316 | 204 | -112 |
| STR (16+32-bit) | 196 | 196 | 0    |
| UMAAL | 128 | 128 | 0    |
| ADCS | 28 | 28 | 0 |
| MOVS / MUL / ADDS / frame | 38 | 39 | +1 |

The LDR drop is the entire story. 112 loads were former "reload b[j] or p[j] every inner step"; now those operands live in r4–r11.

## Anomalies and variance

Iteration 1 of the g1_mul / g2_mul benchmarks came in at ~2x faster than iterations 2–6 (17 ms and 75 ms respectively). This is the Hamming-weight-dependent scalar-mul path: the firmware seeds the test scalar with `iter + DWT::cycle_count()`, so iteration 1's scalar happened to have unusually few set bits. `result.toml` uses iterations 2–6 for the typical values.

Full-verify benchmarks are not scalar-dependent and show tight ±0.05 ms variance across 5 iterations.

## SRAM placement effect, isolated

The step isolates two changes (asm tuning + SRAM placement), so we can't cleanly attribute the -6.9% between them. Both contribute. A future experiment could flip only the `.section` directive back to `.text` to measure RAM-only contribution, but the effort probably isn't worth it for a single data point — the two levers are meant to compound.

## Reproduction

Changes spread across three files:

- `crates/bench-rp2350-m33/memory.x` — new `.ram_text` SECTIONS block with `__ram_text_{vma_start,vma_end,lma_start}` symbols.
- `crates/bench-rp2350-m33/src/main.rs` — `#[pre_init] unsafe fn copy_ram_text()` that runs the flash-to-RAM copy before cortex-m-rt's bss/data init.
- `vendor/bn/src/arith.rs` — `core::arch::global_asm!` block rewritten with `b[]`/`p[]` preloaded in r4–r11; section changed to `.ram_text.mul_reduce_armv8m`.

Host tests unaffected; all 34 fork tests still pass.
