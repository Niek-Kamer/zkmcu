# 2026-04-23 — M33 fork baseline (post-patch-swap, no code changes)

First run after wiring `[patch.crates-io]` to route `substrate-bn` through the maintained fork at https://github.com/Niek-Kamer/bn (tracking `paritytech/bn` master @ `63f8c587`). No firmware code changed, no substrate-bn code changed — this is just a control run to lock in the "fork-unmodified" number that every future asm / linker experiment will compare against.

## Headline

**Groth16 verify: 962 ms (square circuit), median of 3.** That is ~2.6% faster than the 2026-04-21 crates.io-0.6.0 baseline (988 ms), with no work done on our side.

## Deltas vs crates.io-0.6.0 baseline

| Bench                      | crates.io 0.6.0 | fork master | Δ      |
|----------------------------|-----------------|-------------|--------|
| groth16_verify (square)    | 988 ms          | 962 ms      | -2.6%  |
| groth16_verify (sq5)       | not measured    | 974 ms      | —      |
| groth16_verify (semaphore) | not measured    | 1176 ms     | —      |
| pairing (single)           | 533 ms          | 524 ms      | -1.7%  |
| g1_mul (typical)           | 110 ms          | 60 ms       | **-46%** |
| g2_mul (typical)           | 210 ms          | 210 ms      | flat   |

`.text` footprint dropped from 75,180 → 75,156 bytes (-24 bytes). Paritytech/bn master contains commits post the 0.6.0 tag that explain the small changes in binary shape and the 1-3% improvements on verify / pairing.

The **-46% on g1_mul** is outside anything wich could be explained by a few post-release cleanups. Two hypotheses, not yet distinguished:

1. An actual change in the G1 scalar-multiplication code path between 0.6.0 and master (e.g. windowed / wNAF recoding). Would need a diff of `bn::groups::G::mul` against the 0.6.0 tag to confirm.
2. A subtle change in how our firmware picks the benchmark scalar, wich moved it off a high-Hamming-weight path onto a low-Hamming-weight path. Unlikely because firmware itself didn't change, but worth ruling out.

Not investigated today. Flagged for the next session.

## Run shape

User Ctrl+C'd mid-iteration 4 after `[4] pairing start`. Complete samples:

- `g1_mul` / `g2_mul`: 4 iterations each
- `pairing` / `groth16_verify*`: 3 iterations each

Per-iteration variance is tight: verify medians within ±50 μs across iterations, pairing within ±350 μs. No need for longer runs for these benches.

Stack peak grew slightly vs the opt=3 run (15604 vs 15276 bytes). Not a concern (16 KB stack reservation, plenty of margin) — likely just different inlining landing different frame sizes.

## What this means for next experiments

This is the anchor. Every subsequent benchmark run that exercises the forked substrate-bn should be compared against this run, not the 988 ms crates.io number. The 26 ms head start is "free" — unrelated to any optimization we do, so crediting it to our work would misrepresent the wins.

The two live optimization levers (see `.claude/findings/2026-04-23-no-umaal-codegen.md`):

1. **RAM-linked hot `.text`** to sidestep the RP2350's 16 KB XIP cache. One linker-script change, can be exercised without modifying the fork's source.
2. **Hand-written ARMv8-M asm for `U256::mul`** in the fork, using UMAAL. Multi-session, requires constant-time discipline and cross-checks against arkworks.

## Reproduction

`Cargo.toml` `[patch.crates-io]` entry (current):

```toml
substrate-bn = { git = "https://github.com/Niek-Kamer/bn", branch = "master", package = "substrate-bn" }
```

Then:

```bash
cargo build --release          # resolves + compiles the fork
just test                      # arkworks cross-check must pass
just build-m33                 # firmware
scp target/thumbv8m.main-none-eabihf/release/bench-rp2350-m33 \
    pid-admin@10.42.0.30:/tmp/bench-m33.elf
# On Pi 5 with Pico in BOOTSEL:
picotool load -v -x -t elf /tmp/bench-m33.elf
cat /dev/ttyACM0 | tee raw.log
```

For reproducibility, consider pinning to the exact rev rather than following `branch = "master"`:

```toml
substrate-bn = { git = "https://github.com/Niek-Kamer/bn", rev = "63f8c587", package = "substrate-bn" }
```
