# 2026-04-22 — Peak heap usage on Cortex-M33

Measured peak simultaneous heap allocation during one Groth16 verify call by wrapping `embedded_alloc::LlffHeap` in a `TrackingHeap` that updates an `AtomicUsize` peak counter on every successful `alloc()`. Peak is reset to current-usage right before each measured verify so the figure represents *only* that call's workspace, not cumulative state.

## Headline

**Peak heap during one Groth16 verify: 81,280 bytes ≈ 79 KB.**

Identical for the 1-public-input (`square`) and 5-public-input (`squares-5`) circuits. The vk_x linear combination allocates next to nothing compared to the pairing workspace — even adding four extra public inputs doesn't move the peak. `pairing_batch`'s intermediate `Fq12` accumulators and final-exponentiation scratch space dominate.

## Full RAM budget

| Region | Bytes | Notes |
|--------|-------|-------|
| `.text` | 73,432 | code |
| Peak stack | 15,604 | from stack painting |
| Peak heap | 81,280 | from this run |
| Heap base (parsed vk+proof+public) | 960 | committed before verify starts |
| Other static (bss minus heap) | ~44 | atomic counters, linker overhead |
| **Total RAM in use during verify** | **~98 KB** | peak simultaneous |

## What this means for the MCU market

| SRAM class | Fits? | Common chips |
|-----------|-------|--------------|
| 32 KB | no | ATmega32, most Cortex-M0+ |
| 64 KB | no | STM32F030, low-end Cortex-M0+ |
| **128 KB** | **yes, with margin** | nRF52832, STM32F405, ST33 (Ledger), most secure elements |
| 256 KB | easily | nRF52840, STM32F4 high-end |
| 520 KB (RP2350) | trivially | our target |

At ~98 KB of dynamic RAM, zkmcu's unmodified verifier fits on the **128 KB SRAM class** — which covers most hardware-wallet MCUs and a large part of the secure-element market.

"Fits on 64 KB SRAM" would require either:

- Forking `substrate-bn` to reduce pairing-batch memory (avoid eager `Fq12` accumulator allocation; use streaming Miller loop state).
- Switching to a verifier without pairings — Nova / HyperNova native verifiers use one scalar multiplication and a hash, so their RAM budget is tiny.
- Using a pairing implementation that holds more state in stack rather than heap (several academic MCU BLS12-381 pairings do this).

All three are legitimate follow-ups. None fit the Day 3 scope.

## Arena sizing

Default arena was 256 KB — 3.2× larger than needed. A tighter arena of 96 KB leaves ~18 % margin above peak and is the basis for the follow-up run `2026-04-22-m33-heap-96k-confirmed`.

Absolute minimum safe arena = peak + fragmentation margin. Linked-list first-fit allocators typically fragment 10-20 % under mixed-size allocation patterns. 96 KB = peak × 1.21, which is the tight end of comfortable.
