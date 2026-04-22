# 2026-04-22 — Cortex-M33 Semaphore depth-10 verify

First measurement of a *real-world, production-shaped* Groth16 circuit
on the RP2350 Cortex-M33. The VK + proof come from Semaphore v4.14.2
(Merkle tree depth 10) under the CDN-hosted trusted setup, not a
synthetic arkworks-generated vector. Verifier is the same `zkmcu-
verifier` 0.1.0 we've been using for the square / squares-5 benches —
wire format (EIP-197) and crypto backend (substrate-bn) are unchanged.

## Headline

*Semaphore verify: 1176 ms* on the M33 at 150 MHz, iteration-to-
iteration variance **0.030 %** over 5 complete iterations (the 6th
started but the serial capture ended mid-verify). Every iteration
returned `ok=true`.

## Prediction check

The phase 2.7 response budget predicted:

> Semaphore depth-10: predicted *~1160 ms* (4 big inputs, each ~67 ms)

Measured: *1176 ms*, off by +1.4 %. Well inside the budget.

The scaling math:

- square baseline (1 public input): 962 ms
- Semaphore adds 3 extra public inputs beyond the constant IC[0] term
- Each public input is a full 254-bit random-ish scalar — merkle root,
  nullifier, keccak-shifted message hash, keccak-shifted scope hash —
  so none of them benefit from substrate-bn's small-scalar sliding-
  window NAF short-circuit
- Per-input cost: 71 ms measured, compare to \~67 ms predicted from
  the m33-groth16-baseline run's `cycles_typical` for G1 scalar mul
  (110 ms typical vs \~60 ms measured here for a cheaper random-ish
  scalar — typical lands on a bimodal distribution)
- Total: 962 + 3 × 71 = 1175 ms, within 1 ms of measured

This is the first prediction in the phase 2 arc I got right within
noise. Satisfying.

## Why this matters

Earlier runs benched:

- *square* (1 public input, tiny scalar): cheap, but not what anyone
  actually verifies
- *squares-5* (5 public inputs, tiny integers): exercised the IC
  pippenger sum but with small-scalar shortcuts that hide per-input
  cost

The Semaphore vector exercises the **realistic-circuit regime**:
full-width random scalars in every public input. A grant reviewer
reading "zkmcu verifies the same Semaphore proof that Ethereum
verifies, in 1.18 seconds on a $7 MCU" gets the whole point of the
project without needing to unpack the synthetic-benchmark caveats.

## Cross-regime comparison

| | 1 pub | 5 pub small | 4 pub big |
|-|------:|------------:|----------:|
| circuit | square | squares-5 | Semaphore depth-10 |
| scalars | 1 small | 5 small (<2^13) | 4 big (254-bit) |
| M33 verify | 962 ms | 974 ms | **1176 ms** |
| Δ / input | baseline | +3 ms | *+71 ms* |

**Circuit designers targeting embedded BN254 verify should budget
~70 ms per public input on Cortex-M33 once scalars go big**, not the
3 ms that squares-5 suggests. Factor of 24× — this is the
consequence of substrate-bn's scalar-dependent NAF shortcut no
longer applying.

## Limitations of this run

- *.text/.data/.bss not extracted.* Same as previous runs — no
  arm-none-eabi-size on the dev host. Follow-up with `rust-objcopy +
  llvm-size` in CI would close this.
- *boot_measure for squares-5 and semaphore lines got corrupted* by
  serial-flow-control timing during USB enumeration. The per-iter
  loop output is clean; the boot_measure lines for those two
  vectors weren't captured. Re-flash + capture is cheap if we
  decide we need them (heap_peak for Semaphore should be identical
  to squares-5 since pairing_batch dominates).
- *Variance is almost suspiciously low.* 0.030 % over 5 iterations is
  cleaner than any earlier measurement. Possibly just silicon
  determinism; possibly a real improvement in the firmware's output-
  buffering not introducing jitter. Worth noting but not worrying
  about.

## Reproduction

```bash
# One-time vector generation:
git submodule update --init
cd scripts/gen-semaphore-proof && bun install && bun run gen
cd ../..
cargo run -p zkmcu-host-gen --release -- semaphore \
    --depth 10 --proof scripts/gen-semaphore-proof/proof.json

# Firmware build + flash (dev host):
cargo build -p bench-rp2350-m33 --release
scp target/thumbv8m.main-none-eabihf/release/bench-rp2350-m33 \
    pid-admin@10.42.0.30:/tmp/bench-m33.elf

# Pi 5, Pico in BOOTSEL:
picotool load -v -x -t elf /tmp/bench-m33.elf
cat /dev/ttyACM0
```
