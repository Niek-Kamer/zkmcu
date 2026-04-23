# 2026-04-24 — Hazard3 RV32 STARK verify, Quadratic + BumpAlloc

Companion to `2026-04-24-m33-stark-fib-1024-q-bump`. Same proof, same
AIR, same firmware shape aside from the ISA. Both halves of the phase
3.2.y experiment ran back-to-back on the same Pico 2 W.

## Headline

**82.17 ms median**, **0.084 % IQR variance**, 22 clean iterations.
All `ok=true`. Variance at silicon baseline, matching the M33 bump-alloc
result almost exactly.

## The two unexpected wins

The M33 bump run had two headline findings beyond the variance drop
(faster median, smaller proof-size scaling). RV32 has the same two,
amplified:

*Median dropped 10.2 ms* vs phase 3.2.x LlffHeap clone-hoisted. That's
11 % of the previous verify time, gone. M33 only gained 1.7 ms from
the same change. So winterfell's ~400 verify-time allocations were
paying ~25 μs each on Hazard3's `LlffHeap` path vs ~4 μs on
Cortex-M33's. Linked-list first-fit traversal costs Hazard3 more —
probably a mix of weaker branch prediction on the pointer chase and
less aggressive caching than the M33's MPU-backed SRAM access pattern.

*Cross-ISA ratio compressed to 1.21×.* Phase 3.1 under `None` was
1.46×; phase 3.2 under `Quadratic` was 1.33×; phase 3.2.y under
`Quadratic + bump` is 1.21×. Every time we strip away overhead that
isn't pure crypto work — field extension, clone alloc, free-list
traversal — the two ISAs get closer. The residual 21 % gap is the
*pure crypto* gap: Blake3 compressions + Goldilocks extension-field
arithmetic. That's the actual number to quote when comparing cores.

## Variance

Matches M33 almost exactly:

| Measure | RV32 value | M33 value |
|---|---:|---:|
| min-max / median | 0.327 % | 0.352 % |
| IQR / median | **0.084 %** | **0.082 %** |
| std-dev / mean | 0.076 % | 0.080 % |

Both ISAs at ~0.08 % typical variance — in the ~0.03–0.1 % band that
allocation-free pairing verifiers produce on this silicon. Allocator
is no longer the dominant noise source.

The min-max spread is mostly from 1–2 outlier iterations per 20-ish
samples on both ISAs. Same pattern → likely the same underlying cause
(cache / USB peripheral hiccups, not something workload-specific).

## Memory

`heap_peak` = 252,699 B on RV32, vs M33's 314,504 B. Somewhat less
bump-arena usage — roughly 62 KB less. Probably different alignment
padding accumulating and different Vec-realloc ordering between the
two codegen outputs. Both stay comfortably inside the 384 KB arena.

`stack_peak` = 5,544 B, essentially identical to M33's 5,680 B.

## Reproduction

```bash
cargo build -p bench-rp2350-rv32-stark --release
scp target/riscv32imac-unknown-none-elf/release/bench-rp2350-rv32-stark \
    pid-admin@10.42.0.30:/tmp/rv32bump.elf

picotool load -v -x -t elf /tmp/rv32bump.elf
cat /dev/ttyACM0
```
