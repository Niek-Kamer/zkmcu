# 2026-04-23 — Cortex-M33 STARK Fibonacci verify

First on-device STARK verify measurement in the zkmcu arc. Proof is a
`winter-prover`-generated Fibonacci trace at N = 1024 steps, serialised
to 25,332 bytes, verified on a Raspberry Pi Pico 2 W Cortex-M33 core at
150 MHz via `zkmcu-verifier-stark::fibonacci::verify`.

## Headline

**43.8 ms** median verify over 17 iterations. Iteration-to-iteration
variance *0.30 %*. Every iteration returned `ok=true`.

## Prediction check

Predicted in `research/reports/2026-04-23-stark-prediction.typ`
(committed *before* this measurement ran):

| | Predicted | Measured | Δ |
|-|---:|---:|---|
| Verify time | 150-400 ms | 43.8 ms | **~3.5x faster, below band** |
| Peak heap | 120-180 KB | 70.7 KB | **-44 %, below band** |
| Peak stack | 18-30 KB | 5.2 KB | **-75 %, way below band** |
| Proof size | 40-80 KB | 25.3 KB | **-49 %, already noted in phase 3.1.2** |
| Variance | 0.03-0.1 % | 0.30 % | higher than predicted |

Three of the falsification criteria fired. All in the "better than
expected" direction on verify time / memory / proof size; the fourth
(cross-ISA ratio) is discussed in the companion RV32 notes.

## Why my prediction was pessimistic

Every component of the cost budget in the prediction report was too
high:

- *Blake3 cost*. Estimated 45 us / 64-byte compression. Implied real
  figure is closer to 15-25 us based on the measured total. Blake3's
  Cortex-M implementation benefits from the core's 32-bit
  ADD/XOR/ROT pipeline more than the docstring extrapolation suggested.
- *Hash count*. Estimated ~1200 compressions total. At 63-bit conjectured
  security with winterfell's default parameters, 32 queries x ~13 FRI
  layers plus trace-commitment auth paths comes out closer to 500-700
  compressions.
- *Goldilocks field mul*. Estimated 30-50 cycles. On Cortex-M33 with
  UMULL + carry-chain, a 64-bit schoolbook mul is probably closer to
  15-25 cycles.
- *Low security threshold* (63-bit vs the 96-bit example in the
  winterfell docstring) means fewer queries needed — the 32 queries we
  run is the minimum winterfell enforces, not the higher count the
  prediction model assumed.

All four factors compound. 3.5x miss is the result.

## Memory layout

- `heap_base` = 50,686 B: already allocated when the measurement starts.
  This is the parsed `Proof` sitting live — winterfell's `Proof::from_bytes`
  unpacks the 25 KB wire format into a ~50 KB structured form.
- `heap_peak` = 70,739 B: actual peak during one verify call. Δ above
  `heap_base` = 20 KB of verify-time scratch (FRI queries, Merkle auth
  paths, composition poly OOD evals).
- `stack_peak` = 5,200 B: remarkably small. winterfell's verifier doesn't
  build big stack frames — most temporary state goes through the heap
  allocator.

Total RAM during verify: 70,739 + 5,200 + ~500 (statics) ≈ **76 KB**.

## 128 KB SRAM tier consequence

This was the single most consequential uncertainty in the phase-3.1.0
prediction report. The report explicitly said:

> Total RAM of 120-180 KB fits on the 256 KB SRAM tier but likely not
> on the 128 KB tier that Groth16 / BN254 sits on. This is the most
> consequential uncertainty — if STARK verify needs more than 128 KB
> total RAM, the deployment story changes.

*Measured total RAM is 76 KB, well under the 128 KB tier.* The
deployment story does not change: **all three verifier families
(BN254, BLS12-381, STARK) fit on the same hardware-wallet-grade
silicon**. nRF52832, STM32F405, Ledger ST33, Infineon SLE78 — they're
all viable targets for any of the three curves / proof systems.

This is the phase-3 narrative-preserving result. Grant pitch line:
"zkmcu is the first open no_std family of SNARK and STARK verifiers
that all fit on 128 KB SRAM."

## Variance anomaly

0.30 % vs predicted 0.03-0.1 %. Factor of 3-10x worse than the silicon
baseline we saw on BN254 / BLS12 / Semaphore runs. Most likely cause:
`proof.clone()` is called inside the timed window (winterfell's verify
takes `Proof` by value, consuming it). The clone allocates Vec
buffers; the allocator path introduces jitter the pairing-based
verifiers don't have because they don't allocate mid-verify.

Easy to test: move the clone outside the timed window, measure again.
Phase-3.2 cleanup if it matters.

## Cross-curve comparison on same silicon

All measured on this exact Cortex-M33 at 150 MHz:

| Circuit | Verify | Ratio to STARK |
|---|---:|---:|
| STARK Fib-1024 | **43.8 ms** | 1x |
| BN254 Groth16, 1 pub input | 962 ms | 22x slower |
| BN254 Semaphore depth 10 | 1,176 ms | 27x slower |
| BLS12-381 Groth16, 1 pub input | 2,015 ms | 46x slower |

STARK verify is the fastest on this hardware by a wide margin. Makes
sense (no pairings, no Fq12 tower arithmetic) but the magnitude is
striking. The tradeoff is proof size:

| Proof system | Wire proof size |
|---|---:|
| BN254 Groth16 | 256 B (constant) |
| BLS12-381 Groth16 | 512 B (constant) |
| Semaphore Groth16 | 256 B (BN254 underneath) |
| STARK Fib-1024 | **25,332 B** (variable, grows with N) |

Classic STARK throughput-for-bandwidth tradeoff: faster verify, much
bigger proof. On the 96-bit-security variant (phase 3.2, `FieldExtension::Quadratic`)
the proof size would roughly double and verify would ~2x; still
sub-100 ms.

## Limitations of this run

- 63-bit conjectured security (set at prover-side with Goldilocks +
  `FieldExtension::None`). Production uses of STARK typically target
  100+ bits via field extension. Phase 3.2 will run the same AIR with
  `FieldExtension::Quadratic` for ~128-bit security and re-bench —
  verify cost will go up, but per the prediction model it stays well
  under 150 ms on M33.
- `.text` / `.data` / `.bss` not extracted. Same limitation as earlier
  phase runs.
- First few iterations (1-3) got clipped by USB-CDC flow-control
  corruption during enumeration. 17 complete samples from iteration 4
  onward. Enough for a reliable median.
- `proof.clone()` happens inside the timed window. Real-world firmware
  might structure this differently (e.g. reparse fresh bytes each
  verify). Worth noting but not a blocker.

## Reproduction

```bash
# One-time: generate the Fibonacci proof.
cargo run -p zkmcu-host-gen --release -- stark

# Flash + measure on hardware.
cargo build -p bench-rp2350-m33-stark --release
scp target/thumbv8m.main-none-eabihf/release/bench-rp2350-m33-stark \
    pid-admin@10.42.0.30:/tmp/bench-m33-stark.elf

# Pi 5, Pico in BOOTSEL:
picotool load -v -x -t elf /tmp/bench-m33-stark.elf
cat /dev/ttyACM0
```
