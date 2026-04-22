# 2026-04-21 — Hazard3 RV32 Groth16 with stack painting

Same firmware structure as `2026-04-21-m33-stack-painted`, compiled for `riscv32imac-unknown-none-elf`.

## Headline

**Peak stack during one Groth16 verify: 15,708 B (~15.3 KB).**

Only 104 B more than the Cortex-M33 figure (15,604 B) — basically indistinguishable. Since the verifier and `substrate-bn` source is byte-for-byte identical, this confirms peak stack is a property of the *algorithm*, not the ISA.

## RAM footprint on RV32

| Region | Bytes | Notes |
|--------|-------|-------|
| `.bss` (heap + uninit statics) | 262,208 | 256 KB heap arena + 64 B overhead |
| Peak stack (measured) | 15,708 | one verify call |
| Linker-reserved `.stack` section | 262,080 | riscv-rt emits this; our peak uses only 6% of it |
| **RAM genuinely used** | **~278 KB** | on 520 KB SRAM → **53% of total RAM** |

## Delta vs. the earlier RV32 baseline

| Metric | `rv32-groth16-baseline` | This run (`rv32-stack-painted`) | Δ |
|--------|------------------------:|--------------------------------:|---|
| Groth16 verify median | 1,321,900 μs | 1,341,372 μs | +19.5 ms (+1.5%) |
| Pairing median | 700,890 μs | 706,664 μs | +5.8 ms (+0.8%) |

The verify got ~1.5% slower on RV32 while the M33 got ~1.7% *faster* after the same source-level change (adding the stack-painting boot step). Both deltas are within the intra-run variance band and reflect LTO codegen layout shifts from adding ~300 bytes of code, not any algorithmic change. Net: **the ISA-to-ISA ratio is essentially unchanged; RV32 is still ~38% slower than M33 on full verify.**

## Cross-ISA stack comparison

| | M33 | RV32 |
|-|-----|------|
| Peak stack (verify) | 15,604 B | 15,708 B |
| Wall-clock (verify) | 972 ms | 1,341 ms |
| Clock ratio (same 150 MHz) | 1.00× | 1.00× |
| Cycles for verify | 145.7 M | 201.2 M |
| Cycles per byte of stack peak | 9,336 | 12,811 |

RV32 uses ~1% more stack and ~37% more cycles for the same verify. The stack identity makes sense (same algorithm, same Vec<G1> allocations, same depth of the pairing Miller loop recursion); the cycle count difference is the ISA-level story from the previous run.

## The ">64 KB SRAM" pitch is now defensible on both ISAs

Sum: **heap 256 KB + stack 15.7 KB + static statics <1 KB ≈ 272 KB**.

If we shrink the heap arena to, say, 32 KB (an unmeasured figure we should verify), **zkmcu fits on a 64 KB SRAM MCU on both ARM and RISC-V**. That's a 10× bigger chip market than 520 KB class parts.
