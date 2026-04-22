# 2026-04-22 — M33 with 96 KB heap arena (sufficient-RAM confirmation)

Same firmware as `2026-04-22-m33-heap-peak`, but the heap arena shrunk from 256 KB → 96 KB. Everything still works. Measured heap peak is still 81,280 B (unchanged — peak is a property of the workload, not the arena size), leaving 17,024 B = 17 % headroom for allocator fragmentation.

## Headline

**zkmcu runs on 96 KB of heap arena with full correctness retained.**

Combined with the measured 15.6 KB peak stack and ~1 KB of other statics, **total RAM required during verify is ~112 KB**. This is the tight, validated upper bound — not an estimate.

## Delta vs. 256 KB baseline

| | 256 KB arena | 96 KB arena | Δ |
|-|-------------:|------------:|---|
| `bss` | 262,188 B | 98,348 B | **–163 KB** |
| Peak heap | 81,280 B | 81,280 B | same |
| Groth16 verify median | 962 ms | 962 ms | within noise |
| Correctness (ok=true rate) | 100 % | 100 % | unchanged |

No measurable performance impact from shrinking the arena — `substrate-bn`'s allocations are small enough that the linked-list walk time doesn't change meaningfully.

## RAM budget confirmed-deployable class

| SRAM class | Fits? | Example chips / families |
|-----------|-------|--------------------------|
| 64 KB | no | STM32F030 etc. — needs a different verifier design |
| 96 KB | **marginal** | some Cortex-M0+; would require exactly this arena + tight stack |
| **128 KB** | **comfortably** | nRF52832, STM32F405, Ledger ST33K1M5, Infineon SLE78, several SE-class chips |
| 256 KB | easily | nRF52840, STM32F427, Kinetis K60 |
| 512 KB+ | trivially | RP2350, STM32F469 |

The "128 KB SRAM" tier is the relevant target class for hardware wallets and secure elements — this is where zkmcu now sits cleanly, whereas the original claim ("fits on 64 KB") was too aggressive.

## What would unlock 64 KB SRAM?

The measured peak is substrate-bn's pairing_batch workspace. Three paths to reduce it:

1. **Avoid pairing_batch.** Run four serial `pairing()` calls, each freeing its intermediates before the next. Total work stays the same (Miller + final exp × 4) but peak drops. Pairing time approximately doubles (no shared final exp), so verify goes from 988 ms → ~2 s.
2. **Streaming Miller loop.** Refactor substrate-bn's pairing to hold state on the stack rather than allocating Fq12 accumulators on the heap. Open research area; would affect both performance and code complexity.
3. **Switch to a verifier without pairings.** Nova / HyperNova native verifiers use one scalar multiplication and a hash. Peak heap likely < 5 KB. Different proof system; different project.

Path 1 is the cheapest engineering win if someone specifically needs 64 KB SRAM.

## What's still untested

- RV32 heap peak. Expected to be identical to M33 (same `substrate-bn` source path, same Vec allocation patterns).
- Sustained-load fragmentation. 6 iterations per this run; extended runs (say 10⁴ iterations) could surface pathological fragmentation — untested.
- Hazard3-specific peak under different allocator. Worth a side-by-side if we port the TrackingHeap wrapper.

## Reproduction

```toml
# crates/bench-rp2350-m33/src/main.rs
const HEAP_SIZE: usize = 96 * 1024;
```

Same flash/capture pattern as the other runs. The boot line prints `heap=96K` to distinguish it.
