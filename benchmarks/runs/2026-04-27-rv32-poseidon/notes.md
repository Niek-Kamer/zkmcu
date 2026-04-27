# 2026-04-27 — RV32 Groth16 Poseidon Merkle membership (BN254)

RV32 counterpart to the M33 Poseidon run. Same circuits, same vectors, same question: does circuit complexity affect verify cost? Same answer: no.

## Headline

**Depth-3 and depth-10 are 877 µs apart on Hazard3. Same succinctness result as M33, confirmed on a second ISA.**

## Cross-ISA comparison

| circuit | M33 (ms) | RV32 (ms) | ratio |
|---|---:|---:|---:|
| square | 541 | 1,174 | 2.17× |
| poseidon depth-3 | 570 | 1,241 | 2.18× |
| poseidon depth-10 | 570 | 1,242 | 2.18× |
| semaphore depth-10 | 660 | 1,444 | 2.19× |

The ratio is ~2.18× across every circuit without exception. M33 advantage is entirely from the UMAAL asm (`mul_reduce_armv8m` running from SRAM). Hazard3 runs the pure-Rust field multiplication. The speedup factor is constant because the dominant cost — field multiplication in the Miller loop — is the same code path for every circuit regardless of what you're verifying.

## Succinctness delta both ISAs

| | M33 delta (d10 - d3) | RV32 delta (d10 - d3) |
|---|---:|---:|
| µs | 537 | 877 |
| % | 0.09% | 0.07% |

Both are noise. The slightly larger absolute delta on RV32 is consistent with 2.18× slower overall — any residual difference in circuit-dependent work scales proportionally.

## Cost model confirmed cross-ISA

Same breakdown as M33: poseidon costs ~67 ms more than square on RV32 (vs 29 ms on M33), wich is exactly 2.18× × 29 ms = 63 ms — again entirely the `vk_x` scalar mul for a 254-bit public input vs a tiny one.

## Footprint

| metric | value |
|---|---|
| `.text` | 78,620 B |
| heap peak | 82,336 B |
| stack peak | 15,692 B |
| heap configured | 256 KB (RV32 uses larger heap; no UMAAL so no .ram_text in heap region) |
