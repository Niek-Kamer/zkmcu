# 2026-04-22 — Hazard3 RV32 BLS12-381 Groth16 baseline

Companion to `2026-04-22-m33-bls12-baseline` — same silicon, same clock,
same firmware shape, running on the RP2350's RISC-V core instead of the
ARM core. Same committed BLS12-381 vectors. First BLS12-381 / Hazard3
pairing-grade-arithmetic measurement that I'm aware of in public form.

## Variance

7 iterations for the primitives and Groth16 verify; 6 complete iterations
for `groth16_verify_sq5` (capture was Ctrl+C'd during iter 7 while the
~11 s sq5 was still in flight). Variance on the full Groth16 verify is
*331,552 cycles* on a 772.6M median — **0.043 %**, cleaner than the M33
BLS12 run (0.064 %) and the BN254 RV32 run (0.029 %). Every completed
iteration returned `ok=true`.

## Cross-ISA comparison (RP2350 @ 150 MHz, same firmware except arch)

| Operation | Cortex-M33 BLS12 | Hazard3 RV32 BLS12 | RV32 / M33 | RV32 / M33 (BN254 was) |
|-----------|-----------------:|-------------------:|-----------:|-----------------------:|
| G1 scalar mul  | 847 ms   | 1,427 ms  | **1.69×** | 0.65× (RV32 won on BN254) |
| G2 scalar mul  | 523 ms   | 1,003 ms  | **1.92×** | 1.35× |
| Pairing        | 607 ms   | 1,975 ms  | **3.26×** | 1.33× |
| Groth16 verify | 2,015 ms | 5,151 ms  | **2.56×** | 1.39× |
| Stack peak     | 19.4 KB  | 20.6 KB   | 1.06× | 1.01× |

## The headline finding: BN254's RISC-V advantage does not generalize

On BN254, Hazard3 ran G1 scalar multiplication ~35 % faster than
Cortex-M33 — a surprising result flagged in `research/prior-art/main.typ`
and the first-session report. On BLS12-381, Hazard3 is ~69 % *slower*
than Cortex-M33 on G1.

Speculative mechanism: Cortex-M33's `UMAAL` instruction produces a
32×32→64-bit product with a two-accumulator add in a single cycle.
Hazard3 base RV32IMAC has `mul` + `mulhu` as separate instructions
with no accumulator. On BN254's 8-word Fp, the per-mul-op overhead
matters less; on BLS12-381's 12-word Fp (2.25× more schoolbook
word-muls), the carry-chain sequence per word-mul dominates, and the
31 general-purpose registers no longer compensate.

This is a publishable result in its own right: *cross-ISA comparisons
on pairing-friendly curves are prime-size-dependent, not just
algorithm-dependent*. A conclusion drawn from BN254 measurements does
not transfer to BLS12-381 without verification.

## Pairing gap is much worse than predicted

Predicted pairing RV32/M33 ratio: 1.33× (based on the BN254
relationship). Measured: *3.26×*. That is the single largest
prediction miss in the phase-2 comparison.

The BLS12-381 ate pairing has a shorter Miller loop than BN254 (fewer
non-zero bits in the curve parameter), wich the M33 measurement also
benefits from — the pairing came in at 607 ms rather than the predicted
~1220 ms. On RV32 that tailwind is dwarfed by the field-mul headwind:
12×12 schoolbook word-muls without `UMAAL` pay a fixed per-mul
overhead on every one of the Miller loop's Fp muls, and over 6 degrees
of tower (Fp2 → Fp6 → Fp12) the multiplier compounds.

## Public-input scaling

Δ per extra public input: **1,430 ms** on RV32 vs **848 ms** on M33.
Both almost exactly match the local G1 scalar mul time on each core —
0.2 % spread on M33, 0.2 % on RV32. Same algebraic contract as
BN254 and as the M33 run. The "each extra public input adds exactly
one G1 scalar mul" prediction is now confirmed across
(BN254, BLS12-381) × (Cortex-M33, Hazard3 RV32).

## Limitations of this run

- *Heap peak not measured.* The RV32 firmware uses plain `LlffHeap`
  without the `TrackingHeap` GlobalAlloc wrapper the M33 firmware has.
  Future work: port TrackingHeap to RV32 for heap-peak measurement.
  Expected peak ~79 KB based on the M33 BLS12 run (allocations come
  from the same `bls12_381` crate).
- *.text / .data / .bss not extracted.* Same reason as the M33 run —
  no arm/riscv32 size tool on the dev host.
- *Last iteration truncated.* 6 complete sq5 samples, 7 complete
  non-sq5 samples. Medians computed over the complete samples only.

## Reproduction

```bash
# Dev host:
cargo build -p bench-rp2350-rv32-bls12 --release
scp target/riscv32imac-unknown-none-elf/release/bench-rp2350-rv32-bls12 \
    pid-admin@10.42.0.30:/tmp/bench-rv32-bls12.elf

# Pi 5, Pico in BOOTSEL:
picotool load -v -x -t elf /tmp/bench-rv32-bls12.elf
cat /dev/ttyACM0
```
