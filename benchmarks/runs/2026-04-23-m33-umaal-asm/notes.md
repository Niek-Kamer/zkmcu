# 2026-04-23 — M33 Groth16 with hand-written UMAAL Montgomery multiply

First working ARMv8-M asm implementation of substrate-bn's `mul_reduce`. The asm lives in the fork at `vendor/bn/src/arith.rs` under a `cortex-m33-asm` Cargo feature, gated for `target_arch = "arm"`. On host and with the feature off, the original Rust `mul_reduce_rust` is unchanged — both host tests (32 existing + 2 new u32-SOS cross-checks) pass.

## Headline

**Groth16 verify: 697 ms (square circuit), -29.5% vs the 988 ms crates.io baseline.** Every circuit ran `ok=true` across 6 full iterations. Correctness confirmed.

## Deltas

| Bench                      | crates.io 0.6.0 | fork master | **UMAAL asm** | Δ vs crates.io | Δ vs fork |
|----------------------------|-----------------|-------------|---------------|---------------:|----------:|
| groth16_verify (square)    | 988 ms          | 962 ms      | **697 ms**    | **-29.5%**     | -27.5%    |
| groth16_verify (sq5)       | —               | 974 ms      | 704 ms        | —              | -27.7%    |
| groth16_verify (semaphore) | —               | 1176 ms     | 828 ms        | —              | -29.6%    |
| pairing (single)           | 533 ms          | 524 ms      | 377 ms        | -29.3%         | -28.1%    |
| g1_mul (typical)           | 110 ms          | 60 ms       | 36 ms         | -67.3%         | -40%      |
| g2_mul (typical)           | 210 ms          | 210 ms      | 162 ms        | -22.9%         | -23%      |

## Footprint

| Metric          | Fork-baseline | UMAAL asm | Δ       |
|-----------------|---------------|-----------|---------|
| `.text` (bytes) | 75,100        | **73,184** | -2.6%  |
| `U256::mul`     | 4,116         | 334        | -91.9%  |
| `mul_reduce_armv8m` (asm) | — | 1,744 | new |
| Hot-path total  | 4,116         | 2,078      | -49.5%  |

The hand-written asm is **half the code size** of LLVM's schoolbook.

## Algorithm

Separated Operand Scanning (SOS) on 8 × u32 limbs:

1. **Phase 1:** 8×8 schoolbook `a * b → t[0..16]`, unrolled. Each inner step is exactly one UMAAL: `t[k] := a[i]*b[j] + t[k] + carry`.
2. **Phase 2:** 8 Montgomery reduction rows. For row i: `m = t[i] * inv_low (mod 2^32)`, then `t[i..i+8] += m * modulus` (another UMAAL chain), followed by carry propagation ADDS/ADCS through t[i+8..15].
3. **Output:** `t[8..16]` written back to `this`. The caller (in `impl U256::mul`) handles the final conditional subtract.

128 UMAAL instructions total, zero in the baseline. Instruction mix in `mul_reduce_armv8m`:

| Class | Count |
|-------|-------|
| `ldr` | 316 |
| `str` | 196 |
| `umaal` | 128 |
| `adcs` | 28 |
| `movs` / `mul` / `adds` / frame | 37 |

Memory-bound: every UMAAL step is LDR + LDR + UMAAL + STR. The first pass makes no effort to keep operands in registers across iterations — a straight transliteration of the SOS algorithm.

## Where the win came from

Compared to LLVM's output, every UMAAL replaces a UMULL + two ADCS. On Cortex-M33 that's ~3 cycles → 1-2 cycles per accumulation, across 128 accumulations per Montgomery mul. The full-verify speedup is ~28%, slightly less than the ~33% ceiling implied by the per-primitive math because pairing also contains Fp2 Frobenius, final exponentiation chain, and linear arithmetic wich the asm doesn't touch.

## Not investigated in this run

- **Register scheduling / load reduction.** This is a straight transliteration; keeping b[] or the current t-row in registers across iterations should cut LDR count significantly. Expect another 5-15% on `mul_reduce_armv8m`.
- **RAM-linked hot text.** `mul_reduce_armv8m` is 1744 bytes, easily fits in RAM. Was deferred earlier in favor of this asm; still on the table.
- **Other hot asm functions.** `Fq::inverse` (~2 KB), `Fq12::cyclotomic_squared` (~2 KB) are the next-biggest LLVM blobs left.
- **Algorithmic:** GLV on G1 MSM, final exponentiation chain tuning.

## Reproduction

Fork's Cargo.toml has `cortex-m33-asm = []` feature. Firmware's Cargo.toml enables it:

```toml
substrate-bn = { workspace = true, features = ["cortex-m33-asm"] }
```

Then:

```bash
just build-m33
scp target/thumbv8m.main-none-eabihf/release/bench-rp2350-m33 \
    pid-admin@10.42.0.30:/tmp/bench-m33.elf
# On Pi 5 with Pico in BOOTSEL:
picotool load -v -x -t elf /tmp/bench-m33.elf
cat /dev/ttyACM0 | tee raw.log
```

## Anomalies

- User Ctrl+C'd mid-iteration 7 after `[7] groth16_verify start`. Full samples: 7 iterations of g1/g2/pairing, 6 of the three verify variants. Per-iteration variance is tight (±0.2 ms on verify, ±0.1 ms on pairing).
- `g1_mul` went from 60 ms (fork-baseline) to 36 ms with the asm — a -40% drop that's bigger than the 27% on full verify. This makes sense: G1 scalar mul is dominated by Fp arithmetic (through U256::mul), so the UMAAL speedup lands harder there than in mixed pairing work.
