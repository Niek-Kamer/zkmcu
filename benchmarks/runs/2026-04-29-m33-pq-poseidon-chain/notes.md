# 2026-04-29 m33 PQ-Poseidon-chain — first Plonky3 verifier path on RP2350

## Headline

`pq_poseidon_chain_verify`: **492.22 ms** median across 26 iterations on
the Pico 2 W Cortex-M33 @ 150 MHz. All iterations `ok=true`.
Variance **0.051 %** (max − min over median), the second-tightest
measurement in the project after Semaphore (0.030 %). Stack peak
2 456 B, heap peak 216 KB.

This is the smallest meaningful AIR built with the audited
Poseidon2-`BabyBear` path end-to-end (audited round constants used
for both AIR-side hashing and the FRI / Merkle / challenger
permutation via `default_babybear_poseidon2_16`). Not the headline
PQ-Semaphore AIR — that is depth-10 Merkle membership +
nullifier + scope binding (phase 4.0 step 4). This run is the
de-risk anchor between the scoping doc's predictions and the eventual
headline measurement.

## What this falsifies

`research/reports/2026-04-29-pq-semaphore-scoping.typ` § 5 predicted
*900 -- 1800 ms M33, point estimate 1200 ms* for PQ-Semaphore. The
predictions were *transferable* through this anchor AIR — same field,
same hash, same FRI parameter family, comparable trace size. So a
substantial deviation here is informative even though this AIR is not
the headline.

| Quantity                  | Predicted (PQ-Sem) | Measured (this AIR) | Direction      |
|---------------------------|--------------------|---------------------|----------------|
| Verify on M33             | 900 -- 1800 ms     | 492.22 ms           | falsified low  |
| Proof size                | 15 -- 30 KB        | 88 KB               | over (wide trace) |
| Total heap (peak)         | 80 -- 140 KB       | 211 KB              | over (wide trace) |
| Variance                  | 0.05 -- 0.15 %     | 0.051 %             | inside (low edge) |
| Stack peak                | 4 -- 8 KB          | 2.4 KB              | under          |

The verify-time falsification is the load-bearing one. The two
"over" rows are explainable by this AIR's wide trace
(`VectorizedPoseidon2Air` is ~ 200 columns; the planned PQ-Semaphore
AIR is 24 -- 32 columns) — wider trace means more Merkle openings per
query, which inflates both proof size and verifier working set. We
expect both to drop substantially when the narrow PQ-Semaphore AIR
lands.

## What this means for PQ-Semaphore

Restating the scoping doc framing:

> The headline target is 2--3x slower than the 551 ms Groth16
> baseline. > 4x means PQ-Semaphore on this hardware is impractical.

Measured today: this AIR runs at **492 ms — already 11 % faster than
the 551 ms Groth16/BN254 baseline** (`benchmarks/runs/2026-04-28-m33-bn254-rebench`).
PQ-Semaphore proper has *more queries* (64 vs 28), *more constraint
complexity* (Merkle path verification + nullifier + scope hash inside
the AIR), and more *trace rows* (~256 vs 64), which all push the
number up. But the column-count compounds the *other direction*: a
narrow trace makes per-query work much cheaper. Net direction is
unclear without measuring, but the scoping doc's prediction range now
looks ~2x too pessimistic.

Forward-looking guess (to be re-tested): PQ-Semaphore verify on M33
plausibly lands in *400 -- 1000 ms* range, point estimate ~ 600 ms.
That's not the published prediction — the published prediction is
immutable per project convention. This is a *new* informal estimate
based on the anchor data, to be replaced by a real prediction
report before the PQ-Semaphore measurement.

## Surprises worth flagging

1. **Plonky3 is faster than winterfell on this hardware than the
   scoping doc assumed.** The scoping doc anchored to winterfell's
   measured Fibonacci verify and applied scaling factors. Plonky3's
   verifier appears to have lower per-query overhead than the
   winterfell anchor implied — possibly because Plonky3's modular
   design lets `cargo build --release` LTO inline more aggressively
   than the winterfell umbrella's coarse-grained dep boundaries.
   Worth confirming with a like-for-like measurement: same circuit,
   same params, two verifiers. Phase 4.x territory.

2. **Variance is 0.051 %** despite this being a substantially heavier
   workload than any prior STARK run on this hardware (more deps,
   wider trace, more allocations). TLSF allocator's O(1) alloc/free
   plus bench-core's `measure_cycles` guard rails are clearly
   absorbing the extra setup-vs-measurement noise.

3. **Heap baseline is already ~84 KB before the verify call** — that
   is the parsed `Proof<Config>` plus the precomputed `Config` plus
   the `Air`. The verifying call adds another 132 KB of working set
   on top. Most of this is FRI query-path scratch and Merkle
   verification scratch; the postcard decode of the 88 KB proof
   accounts for the bulk of the baseline 84 KB.

## RV32 measurement

Pending. Same firmware shape, same vector. Expected to land within
±5% of M33 per the phase-3.3 BabyBear cross-ISA pattern (M33 / RV32
ratio of 1.04× on `BabyBear`+Quartic). RV32 firmware is built and
sitting at `target/riscv32imac-unknown-none-elf/release/`.

## Reproducibility

```bash
# Regenerate proof.bin (byte-deterministic, SHA-256 stable):
just regen-vectors  # or cargo run -p zkmcu-host-gen --release -- pq-poseidon-chain

# Build firmware:
just build-m33-pq-poseidon-chain

# Hand-deliver to Pi 5:
scp target/thumbv8m.main-none-eabihf/release/bench-rp2350-m33-pq-poseidon-chain \
    pid-admin@10.42.0.30:/tmp/bench-m33-pq-poseidon-chain.elf

# On Pi 5 with Pico in BOOTSEL:
picotool load -v -x -t elf /tmp/bench-m33-pq-poseidon-chain.elf
cat /dev/ttyACM0
```

## Files

- `raw.log` — captured serial output, 26 iterations.
- `result.toml` — structured results + falsification scorecard against
  the scoping doc.
- This `notes.md`.
