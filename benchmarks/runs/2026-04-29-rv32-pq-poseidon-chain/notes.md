# 2026-04-29 RV32 PQ-Poseidon-chain — first Plonky3 verifier path on Hazard3

## Headline

`pq_poseidon_chain_verify`: **615.87 ms** median across 15 iterations
on the Pico 2 W Hazard3 RV32 @ 150 MHz. All iterations `ok=true`.
Variance **0.076 %**, slightly looser than the M33 sibling's 0.051 %
but inside the predicted band.

Stack peak 1 964 B (tighter than M33's 2 456 B, RV32 ABI saves
fewer registers per call), heap peak 216 KB (identical to M33,
ISA-independent allocation pattern).

## Cross-ISA delta vs M33

| Quantity | M33 | RV32 | RV32 / M33 |
|---|---:|---:|---:|
| Verify (median) | 492.22 ms | 615.87 ms | **1.25×** |
| Variance | 0.051 % | 0.076 % | 1.49× |
| Stack peak | 2 456 B | 1 964 B | 0.80× |
| Heap peak | 216 KB | 216 KB | 1.00× |

The 1.25× cross-ISA ratio **falsifies** the scoping doc's predicted
0.85--1.20× band on the high side (5 % above the upper bound).
Diverges from the phase-3.3 BabyBear+Quartic Fibonacci-on-winterfell
pattern (1.04× ratio) — which is what the prediction was anchored to.

### Most plausible cause

`p3-monty-31` (Plonky3's BabyBear arithmetic crate) uses a
straightforward 32×32 → 64 widen-and-Montgomery-reduce shape.
Cortex-M33 has `UMAAL` (32×32 multiply-accumulate-into-64 in one
cycle) which the compiler emits for that shape; Hazard3 RV32 lacks
an equivalent fused instruction (it has `mulh` for the high half but
no MAC). The wider trace + Poseidon2 hashing in this AIR exercises
BabyBear arithmetic far more heavily per query than Fibonacci's
two-column trace did, exposing the per-multiply gap that previously
got buried under FRI / Merkle commitment work.

To confirm: a like-for-like Fibonacci-on-Plonky3 measurement on both
ISAs would isolate the cause (AIR shape vs verifier framework).
Phase 4.x territory.

## What this falsifies in the scoping doc

| Quantity                    | Predicted (PQ-Sem) | Measured (this AIR) | Direction       |
|-----------------------------|--------------------|---------------------|-----------------|
| Verify on RV32              | 940 -- 1880 ms     | 615.87 ms           | falsified low   |
| RV32 / M33 ratio            | 0.85 -- 1.20×      | 1.25×               | falsified high  |
| Proof size                  | 15 -- 30 KB        | 88 KB               | over (wide trace) |
| Total heap (peak)           | 80 -- 140 KB       | 211 KB              | over (wide trace) |
| Variance                    | 0.05 -- 0.15 %     | 0.076 %             | inside          |
| Stack peak                  | 4 -- 8 KB          | 1.9 KB              | under (good)    |

Two falsifications, both informative:

* **Verify time falsified low** (~ 1.5× faster than the lower bound),
  same direction as M33. PQ-Semaphore proper will land separately
  and may track this anchor's speedup through to the headline AIR.
* **Cross-ISA ratio falsified high.** UMAAL benefit on M33 is more
  visible than the phase-3.3 cross-ISA pattern implied. PQ-Semaphore
  on RV32 will likely show the same or larger M33 advantage.

## Vs the Groth16/BN254 RV32 baseline

| | Groth16 / BN254 | Plonky3 / Poseidon2-chain | Speedup |
|-|---:|---:|---:|
| Verify (M33) | 551 ms | 492 ms | **1.12×** |
| Verify (RV32) | 1 363 ms | 616 ms | **2.21×** |

On RV32 the Plonky3 verifier is **more than twice as fast as the
Groth16 verify** on the same silicon. Why this gap is bigger on RV32
than on M33: Groth16 verify is dominated by BN254 Fq Montgomery
multiplication where M33's UMAAL saves cycles; RV32 lacks that
instruction and pays the full schoolbook cost. Plonky3's BabyBear is
31-bit (one register, no widening) so the per-multiply gap between
M33 and RV32 closes substantially.

Headline shift: the scoping doc's "PQ tax on verify time" framing is
falsified on both ISAs, more strongly on RV32. PQ verify is not a
verify-time regression — it is a proof-size / heap trade.

## Reproducibility

```bash
just regen-vectors        # produces deterministic proof.bin
just build-rv32-pq-poseidon-chain
scp target/riscv32imac-unknown-none-elf/release/bench-rp2350-rv32-pq-poseidon-chain \
    pid-admin@10.42.0.30:/tmp/bench-rv32-pq-poseidon-chain.elf
# Pi 5, Pico in BOOTSEL:
picotool load -v -x -t elf /tmp/bench-rv32-pq-poseidon-chain.elf
cat /dev/ttyACM0
```
