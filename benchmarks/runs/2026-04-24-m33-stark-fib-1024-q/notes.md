# 2026-04-24 — Cortex-M33 STARK Fibonacci verify, Quadratic extension

First production-grade-security STARK verify on RP2350 in the zkmcu arc.
Same AIR as phase 3.1 (Fibonacci, N=1024), same silicon, same clock,
same firmware — only the prover's `ProofOptions` changed from
`FieldExtension::None` to `FieldExtension::Quadratic`. Conjectured
security lifted from 63 bits to 95 bits.

## Headline

**70.0 ms** median verify over 18 iterations. Iteration-to-iteration
variance *0.33 %*. Every iteration `ok=true`. Total RAM ~100 KB,
**well under the 128 KB hardware-wallet tier**.

## The 128 KB tier holds at production security

Phase 3.1 put STARK verify at ~76 KB total RAM under 63-bit
conjectured security. The phase 3.2.0 prediction report flagged
"total RAM stays under 128 KB at Quadratic extension" as *the* most
consequential open question, because a miss in the other direction
would break the grant pitch line "all three verifier families on one
hardware-wallet-tier SRAM budget".

Measured: 93,515 B heap peak + 5,640 B stack peak + ~500 B statics
= **99,655 B ≈ 97.3 KiB total RAM**. 31 KB margin under 128 KB.

The finding survives. BN254 Groth16, BLS12-381 Groth16, and STARK at
production security all fit the same silicon tier (nRF52832,
STM32F405, Ledger ST33, Infineon SLE78).

## Prediction vs measurement

Predicted in `research/reports/2026-04-24-stark-quadratic-prediction.typ`
(committed in commit e872086, *before* the prover was changed):

| | Predicted | Measured | Δ |
|-|---:|---:|---|
| M33 verify | 90–140 ms | 70.0 ms | **20 ms below band** |
| Proof size | 40–50 KB | 30.9 KB | **9 KB below band** |
| M33 heap peak | 110–140 KB | 93.5 KB | **17 KB below band** |
| M33 stack peak | 5.2–7 KB | 5.6 KB | within |
| M33 total RAM | 115–150 KB | ~100 KB | **15 KB below band** |
| Variance | 0.3–0.8 % | 0.33 % | within (low end) |

Four of six predictions fell below the band — same optimistic miss
pattern as phase 3.1.

## Why the model was pessimistic

The phase 3.2.0 cost model assumed:
1. Field arithmetic is ~30 % of verify time under `None`
2. Extension-field ops are ~3× base-field ops (Karatsuba)
3. So Quadratic should grow verify by factor ~1.6× just from the
   field arithmetic portion, plus ~1.1× from the hash work growth

Measured ratio: 70.0 / 43.8 = 1.60×. That matches the *point estimate*
of the model, but the model extrapolated to a band of 2.05×–3.20× when
it should have centered tighter around 1.6×. The band was set too wide
on the pessimistic side because I hedged against "reduction dominates"
and "winterfell uses schoolbook not Karatsuba" — neither of which
materialised.

Proof size growth was smaller than predicted because the proof is
*auth-path-dominated*, not FRI-evaluation-dominated. 32 queries × ~13
layers × 32 B/hash = ~13 KB of unchanged hash auth paths, so even
doubling the FRI evaluations only pushes total up 22 %, not 70 %.

## Memory breakdown

- `heap_base` = 61,798 B (was 50,686 B at `None`): +22 %. Matches
  proof-size growth — parsed proof is resident at start of verify.
- `heap_peak` = 93,515 B (was 70,739 B): +32 %. Verify scratch grew
  more than proof-parsed form, because extension-field state needs
  more live $F_(p^2)$ elements during FRI + DEEP composition.
- `stack_peak` = 5,640 B (was 5,200 B): +8 %. Marginal, as predicted.
  winterfell keeps its state on the heap.

Δ(heap_peak) over Δ(heap_base) = (93515 − 61798) / (70739 − 50686) =
31,717 / 20,053 = 1.58×. Verify-time scratch grew 58 % more under
Quadratic, mostly in FRI state.

## Variance holds

0.33 % vs phase 3.1's 0.30 % — essentially unchanged. The
`proof.clone()`-in-timed-window hypothesis predicted variance would
scale with proof size, but it didn't. That *weakens* the clone
hypothesis. Possible alternative: the allocator jitter is a fixed
cost per `Vec::with_capacity` call rather than a per-byte cost, so
the same number of allocations in roughly the same sizes produces
similar jitter regardless of total proof size.

Still worth pinning down in phase 3.2.x by moving the clone outside
the timed window. Not blocking.

## Reproduction

```bash
# One-time: regen the quadratic-extension proof.
cargo run -p zkmcu-host-gen --release -- stark
# → crates/zkmcu-vectors/data/stark-fib-1024/{proof.bin,public.bin}
# (proof.bin is now 30,888 B, was 25,332 B at FieldExtension::None)

# Flash + measure on hardware.
cargo build -p bench-rp2350-m33-stark --release
scp target/thumbv8m.main-none-eabihf/release/bench-rp2350-m33-stark \
    pid-admin@10.42.0.30:/tmp/m33q.elf

# Pi 5, Pico in BOOTSEL:
picotool load -v -x -t elf /tmp/m33q.elf
cat /dev/ttyACM0
```
