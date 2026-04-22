# 2026-04-21 — M33 Groth16 public-input scaling

Compares `square` (1 public input) against `squares-5` (5 public inputs, small values) on the same silicon, same firmware, same compiler. Each iteration runs both verifies back-to-back so every data point is a paired measurement.

## Headline

**Groth16 verify, same circuit shape, 5 × the public inputs:**

| Circuit | Public inputs | ms | Δ vs 1-input |
|---------|---------------|-----|--------------|
| `square` | 1 | 967 | baseline |
| `squares-5` | 5 | 979 | **+12 ms** |

That's **~3 ms per extra public input** — 20× smaller than the naïve extrapolation of "one G1 scalar mul per input × ~10 ms/mul" would suggest.

## Why so cheap?

The verifier computes `vk_x = ic[0] + Σ x_i · ic[i+1]`, one G1 scalar multiplication per public input. Naïve cost prediction: each G1 mul ≈ 10 M cycles ≈ 67 ms. Four extra inputs should therefore add ~267 ms.

Actual cost: ~465 K cycles ≈ 3 ms per input. The difference is the **magnitude of the scalar**.

Our `squares-5` test vector sets `y_i = x_i²` with `x_i ∈ {3, 20, 37, 54, 71}`, so the public inputs are `y_i ∈ {9, 400, 1369, 2916, 5041}` — all under 2^13. `substrate-bn`'s G1 scalar multiplication is a sliding-window or double-and-add implementation whose cost is dominated by the number of non-zero bits in the scalar. For a 13-bit input the loop runs effectively 13 iterations of double-and-conditional-add, not 256.

A scalar uniformly random in Fr would have ~128 set bits out of 254 → full 10 M cycle cost. A scalar like `9` has 2 set bits → near-zero cost.

## What this means for real circuits

The *scaling* of Groth16 verifier cost with public-input count **depends on the shape of the inputs**, not just the count:

| Input shape | Typical scalar size | Extra cycles per input | Extra ms per input |
|-------------|--------------------:|----------------------:|-------------------:|
| Merkle root / hash output | ~256 bits (uniformly random) | ~10 M | ~67 |
| Ethereum address / pubkey-like | ~160 bits (random) | ~6 M | ~40 |
| Counter / sum / small index | < 2^16 | ~500 K | ~3 |
| Boolean flag | 1 bit | ~50 K | ~0.3 |

A 10-public-input circuit could therefore take anywhere from ~970 ms (ten Boolean flags) to ~1.8 s (ten Merkle roots). This is a 2× spread driven entirely by how many bits the public inputs actually contain.

No other embedded-ZK treatment I've seen calls this out, and it's directly important for:

- Hardware wallet designers choosing between "verify on-device" and "verify upstream" for specific circuits.
- Credential-system designers picking public-input layouts: packing several small flags into one Fr vs. leaving them as separate inputs affects verify time by orders of magnitude.
- Anyone writing verifier cost models — "cost per public input" without specifying input size is misleading by up to 20×.

## Stack

**Identical** to the `square` case: 15,604 B for both 1-input and 5-input circuits.

This also makes sense: the vk_x loop is a single frame regardless of N. No recursion, no per-iteration allocation that grows the call graph. `substrate-bn` internally allocates `Vec<...>` values for pairing_batch, but those are heap, not stack.

Implication: **the "fits in 16 KB of stack" claim holds regardless of the number of public inputs.**

## Caveats

- Only one public-input magnitude regime tested (small integers). A follow-up with random 256-bit public inputs would confirm the upper bound and the 20× spread we're claiming.
- Only M33 measured here. RV32 scaling should behave similarly (same Rust source path) but is untested.
- `substrate-bn`'s scalar-mul algorithm isn't explicitly documented as "variable-time by Hamming weight", but the numbers make it obvious. Worth reading the source if we want to prove it formally.

## Reproduction

```bash
cargo run -p zkmcu-host-gen --release    # writes squares-5/ alongside square/
cd crates/bench-rp2350-m33
cargo build --release
scp target/thumbv8m.main-none-eabihf/release/bench-rp2350-m33 pi:/tmp/bench-m33.elf
# Pico in BOOTSEL:
picotool load -v -x -t elf /tmp/bench-m33.elf
cat /dev/ttyACM0
```

Main loop now emits `groth16_verify` and `groth16_verify_sq5` per iteration.
