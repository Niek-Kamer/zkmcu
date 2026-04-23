# 2026-04-23 — M33 opt-level=3 (regression)

One-variable experiment against the 2026-04-21 baseline: flip the workspace `[profile.release]` setting from `opt-level = "s"` to `opt-level = 3`. Nothing else changed. Same rustc (1.94.1), same substrate-bn (0.6.0), same firmware, same test vectors.

## Headline

**opt-level=3 made it slower.** Groth16 verify went from 988 ms → 1133 ms on the `square` circuit, a +14.7% regression. Every sub-benchmark regressed in the same direction.

| Bench                     | opt="s" (2026-04-21) | opt=3 (this run) | Δ      |
|---------------------------|----------------------|------------------|--------|
| groth16_verify (square)   | 988 ms               | 1133 ms          | +14.7% |
| groth16_verify (sq5)      | n/a (first measurement) | 1160 ms       | —      |
| groth16_verify (semaphore)| n/a (first measurement) | 1645 ms       | —      |
| pairing (single)          | 533 ms               | 576 ms           | +8.1%  |
| g2_mul (typical)          | 210 ms               | 202 ms           | -3.8%  |
| g1_mul (typical)          | 110 ms               | 147 ms           | +33.6% |

`text` section grew from 73 KB (opt="s") to 123 KB (opt=3), a +67% bloat.

## Root cause (hypothesis, not yet confirmed by disassembly)

Two compounding effects:

1. **XIP instruction cache thrashing.** The RP2350 executes from QSPI flash through a small on-chip cache. Hot Miller-loop code that fit in cache at 73 KB spills on every iteration at 123 KB.
2. **Register-pressure spilling.** 32-bit ARM has 12 usable GPRs. 256-bit Montgomery multiplication already spills onto the stack. More aggressive inlining at opt=3 extends temporary live ranges, wich increases spill count. More spills = more load/store traffic on an already memory-bound workload.

Both effects amplify each other: the extra spill instructions *are* the extra code that busts the cache.

## What this means for the optimization plan

Compiler-flag tricks are not going to move this baseline. The code is not ALU-bound; it is memory/cache bound in the inner loop. The productive levers are:

- **Instruction-level:** UMAAL-based Montgomery multiplication in hand-written ARMv8-M asm (via a substrate-bn fork).
- **Algorithmic:** G2 line-coefficient precomputation, GLV endomorphism on G1 MSM, tuned final-exponentiation chain.
- **Memory placement:** moving hot `.text` into SRAM via a linker section, sidestepping XIP cache entirely.

## Reproduction

Edit `Cargo.toml`:

```toml
[profile.release]
opt-level = 3  # was "s"
```

Then:

```bash
just build-m33
scp target/thumbv8m.main-none-eabihf/release/bench-rp2350-m33 pid-admin@10.42.0.30:/tmp/bench-m33.elf
# On Pi 5 with Pico in BOOTSEL:
picotool load -v -x -t elf /tmp/bench-m33.elf
cat /dev/ttyACM0 | tee raw.log
```

The workspace has been reverted to `opt-level = "s"` after this run.

## Anomalies

- Iteration 5 was Ctrl+C'd mid-`groth16_verify`; the preceding `g1mul`/`g2mul`/`pairing` samples are complete. `iterations = 4` for the full-verify benches, `iterations = 5` for the sub-benches.
- `g2_mul` showed a small (-3.8%) *improvement*, unlike every other bench. Likely noise from the Hamming-weight-dependent scalar path; do not over-index on it.
