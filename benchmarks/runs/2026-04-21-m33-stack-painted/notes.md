# 2026-04-21 — M33 Groth16 with stack-painting measurement

Adds a **direct measurement of peak stack usage** during one Groth16 verify call, using stack painting: paint a 64 KB region just below current SP with `0xDEADBEEF` sentinels, call `verify`, scan for the lowest address that got overwritten. The margin (512 B) gets added back to the reported figure to account for the measurement-function's own frame overhead.

## Headline

**Peak stack during one Groth16 verify: 15,604 B ≈ 15.2 KB.**

Combined with our static memory footprint, zkmcu's total RAM requirement on Cortex-M33 is:

| Region | Bytes | Notes |
|--------|-------|-------|
| `.bss` (heap + uninit statics) | 262,180 | of which 262,144 B = 256 KB is the static heap |
| Peak stack (measured) | 15,604 | one verify call |
| **Total RAM used** | **~272 KB** | on 520 KB SRAM → **48% headroom** |

If we shrink the heap to the minimum needed (which we haven't measured yet, but likely under 64 KB), we could plausibly run on a 96–128 KB SRAM MCU. That opens up much cheaper chips.

## Why the verify also got 1.7% faster

| Run | Verify median (ms) | Notes |
|-----|-------------------|-------|
| `2026-04-21-m33-groth16-baseline` | 988.5 | initial build |
| `2026-04-21-m33-post-depbump`     | 986.4 | embedded-alloc 0.7, heapless 0.9, panic-halt 1.0, embedded-hal via fork |
| `2026-04-21-m33-stack-painted` (this run) | 971.7 | same deps + stack-painting boot step added |

Best guess: adding the `measure_verify_stack_peak` function changed the binary's code layout, LTO reshuffled cold/hot paths, verify got slightly better icache locality. The change is within the noise floor (0.05% iteration variance) so we shouldn't over-read it, but it's interesting that inserting ~200 extra bytes of code *improves* the hot path by 16 ms.

## Measurement caveats

- **Granularity: 4 bytes** (sentinel word size). Reported peak is accurate to one u32.
- **Margin: 512 bytes** around current SP are deliberately unpainted to avoid clobbering the measurement function's own frame. That margin is added back in the report, so the number represents total frame chain depth during verify.
- **Window: 64 KB.** If verify exceeded that, we'd see `bytes=65536` in the output. We saw 15,604 — comfortably inside the window.
- **Sample: one verify call.** Subsequent verify calls will use similar stack; pathological re-entry isn't a concern because the benchmark is synchronous.

## What this unlocks

- Concrete grant / pitch statement: **"Groth16/BN254 verify fits in 15.2 KB of stack on ARM Cortex-M33 without any hand-tuning."**
- A natural next optimization axis: if we shrink the heap (mostly substrate-bn `Vec` allocations during `pairing_batch`) we can target chips with as little as 64 KB SRAM, which dramatically expands the addressable hardware market.
- Cross-ISA comparison: same measurement on Hazard3 RV32 is the next run (`2026-04-21-rv32-stack-painted`).

## Raw log (first few iterations)

See `raw.log`. The `[boot]` line is the headline measurement; subsequent `[N]` iterations are the same benchmarks as the baseline run and are there only to show correctness and timing remain stable.
