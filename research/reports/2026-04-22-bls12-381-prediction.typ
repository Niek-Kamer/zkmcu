#import "/research/lib/template.typ": *

// Prediction report, published before any BLS12-381 bench code exists.
// Rationale: see research/notebook/2026-04-22-bls12-381-dep-fit.md.
// Freezing predictions in git before measurement is what makes a later
// benchmark a scientific result rather than a narrative exercise.

#show: paper.with(
  title: "BLS12-381 Groth16 on RP2350: predicted performance (pre-measurement)",
  authors: ("zkmcu",),
  date: "2026-04-22",
  kind: "report",
  abstract: [
    Numerical predictions for extending zkmcu's Groth16 verifier from
    BN254 to BLS12-381, measured on the same RP2350 silicon and
    firmware harness. Published before any BLS12-381 bench code exists
    so the predictions remain falsifiable. Expected full-verify time
    is *2.1--2.4 s on Cortex-M33*, roughly 2.3× the ~962 ms BN254
    baseline. The project's tier-alignment story hinges on one
    uncertain number: peak heap is projected at ~130 KB, which would
    push the verifier off the 128 KB SRAM tier and onto the 256 KB
    tier. Both outcomes are publishable.
  ],
)

= Why this document exists

Measurements without published predictions are not falsifiable: the
reader cannot distinguish reporting from curation. This report pins
the author's expected numbers in git before any Phase 2 firmware
runs. A later benchmark report is then obligated to quote these
predictions and state the measured values next to them.

If these predictions are wrong outside a stated bound, the difference
is itself a finding. That is the point.

This document does not get edited once committed. Errors are
addressed by writing a new dated report that references this one.

= Setup assumptions

The BLS12-381 bench will replicate the BN254 harness as closely as
possible:

- *Hardware.* Raspberry Pi Pico 2 W (RP2350, Cortex-M33 and Hazard3 RV32
  cores, both @ 150 MHz, 520 KB SRAM).
- *Toolchain.* The same `rustc 1.94.1` pin used for the BN254 baseline.
- *Profile.* `release`, `lto = "fat"`, `opt-level = "s"`,
  `codegen-units = 1`, `panic = "abort"`.
- *Crate.* `bls12_381 = "0.8"` (zkcrypto), `default-features = false`,
  features `["groups", "pairings", "alloc"]`. Dep-fit verified in
  `research/notebook/2026-04-22-bls12-381-dep-fit.md`; builds clean on
  both `thumbv8m.main-none-eabihf` and `riscv32imac-unknown-none-elf`
  with zero warnings.
- *Circuits.* The `square` (1 public input) and `squares-5` (5 public
  inputs) families used for BN254, regenerated against
  `ark-bls12-381` + `ark-groth16`. Same constraint count, different
  curve.
- *Measurement methodology.* `DWT::cycle_count` on Cortex-M33, `mcycle`
  on Hazard3 (after the `mcountinhibit` unmask at boot). At least 7
  iterations per datapoint.

= Predictions

== Primitive operations, Cortex-M33

BLS12-381 is a 381-bit prime, BN254 a 254-bit prime. On a 32-bit core
with schoolbook multiplication the word-count is quadratic: 8×8 = 64
word-muls for BN254, 12×12 = 144 word-muls for BLS12-381. Ratio 2.25×,
plus Montgomery-reduction overhead proportional to word count. The
scalar fields are similar in size (254 vs 255 bits), so NAF window
counts are effectively identical. Both curves use an Fq12 tower of the
same degree; per-tower-op cost scales with the underlying Fq.

#compare-table(
  ("Operation", "BN254 (M33, measured)", "BLS12-381 (M33, predicted)", "Rationale"),
  (
    ([Fq mul], [~3 μs], [~7.5 μs], [word-count² ratio, ×2.5]),
    ([G1 scalar mul, typical], [110 ms], [~275 ms], [×2.5 via Fq mul]),
    ([G2 scalar mul, typical], [210 ms], [~525 ms], [×2.5 via Fq2 mul]),
    ([Pairing], [533 ms], [~1220 ms], [Fq12 tower over a larger Fq]),
    ([Verify (1 public input)], [962 ms], [2100--2400 ms], [pairings dominate]),
  ),
)

All "predicted" figures are the author's pre-measurement best
estimates. Expected error is at least 10--20 % per line. The interval
on full verify reflects uncertainty about whether the final
exponentiation on BLS12-381 is heavier or lighter than expected: it
has a shorter easy part but a heavier cyclotomic hard part, and the
net depends on compiler decisions that cannot be predicted from field
sizes alone.

== Hazard3 RV32

On the BN254 baseline, Hazard3 was ~35 % faster than Cortex-M33 on G1
scalar multiplication (register-count advantage on schoolbook
multiplication) and 31--39 % slower on Fq12 tower operations
(allocation / memory overhead). Applying the same multipliers:

#compare-table(
  ("Operation", "BN254 RV32 (measured)", "BLS12-381 RV32 (predicted)", "Ratio"),
  (
    ([G1 scalar mul, typical], [72 ms], [~180 ms], [0.65×]),
    ([Pairing], [707 ms], [~1620 ms], [1.33×]),
    ([Verify (1 public input)], [1341 ms], [2800--3200 ms], [1.33×]),
  ),
)

= Memory

The pairing batch workspace scales with field size. BN254 peak heap
during verify was 81,280 B with a `pairing_batch` of four pairings.
Scaling naively by word-count (1.5×) and accounting for a larger
Fq12 working-set:

#compare-table(
  ("Quantity", "BN254 M33 (measured)", "BLS12-381 M33 (predicted)"),
  (
    ([Peak heap, verify], [81,280 B (~79 KB)], [~130 KB]),
    ([Required arena (17 % margin)], [96 KB], [~160 KB]),
    ([Peak stack], [15,604 B], [~18,000 B]),
    ([Total RAM during verify], [~111 KB], [~180 KB]),
  ),
)

This is the predictions' most consequential uncertainty. The BN254
build fits on the 128 KB SRAM tier, aligning with common
hardware-wallet-grade silicon (nRF52832, STM32F405, Ledger ST33,
Infineon SLE78). A 180 KB BLS12-381 build would push it onto the
256 KB tier. If measured peak heap is under ~100 KB, the 128 KB tier
story is preserved. If it exceeds ~110 KB, the project will need to
either (a) document a serial-pairing path at ~2× verify cost to stay
in 128 KB, or (b) acknowledge the tier shift and pivot the deployment
narrative.

= Falsification criteria

The following outcomes would invalidate the scaling argument above:

- Full verify on M33 outside *1.5--3.5 s*. Below 1.5 s would suggest
  the Fq12 tower on BLS12-381 is cheaper than field-size scaling
  predicts (plausible if LLVM unrolls differently). Above 3.5 s
  would suggest a specific inefficiency in `bls12_381`'s pairing
  that is not present in `substrate-bn`.
- Hazard3 not within ±20 % of the predicted 1.33× RV32 / M33 ratio
  on the full verify. The BN254 observation was that Fq12 tower
  arithmetic is where M33 pulls ahead; if that relationship does not
  hold for BLS12-381 the cross-ISA story needs re-examination.
- Peak heap outside *100--200 KB*. Under 100 KB would suggest
  `pairing_batch` is implemented more tightly than in `substrate-bn`
  (a useful finding). Over 200 KB would suggest an
  allocation-per-op pattern the BN254 crate avoids.

= Wire format choice

Phase 2 uses EIP-2537, the Ethereum BLS12-381 precompile encoding:
64-byte Fp (48 bytes of value, 16 bytes of leading zeros), 128-byte
G1 (`x ‖ y`), 256-byte G2 (`x.c0 ‖ x.c1 ‖ y.c0 ‖ y.c1`). The 33 %
wire bloat vs a tight 48-byte Fp encoding buys compatibility with
on-chain verification, which matters for the "same proof verifies
on-device and on Ethereum" narrative underlying L2 light-client and
bridge use cases.

= Phase 2 deliverables

In build order:

+ This document.
+ Host-side proof generation + wire-format round-trip. Extending
  `zkmcu-host-gen` with a `bls12-381` mode producing EIP-2537 test
  vectors. Cross-checked on host with `bls12_381`'s native verify
  before any firmware touches the bytes.
+ `crates/zkmcu-verifier-bls12`: `no_std` parser + verify wrapper
  mirroring `zkmcu-verifier`'s shape.
+ Adversarial + property-based test suite at parity with
  `zkmcu-verifier/tests/`.
+ `bench-rp2350-m33-bls12` and `bench-rp2350-rv32-bls12` firmware.
+ First measurement run under `benchmarks/runs/<date>-m33-bls12-baseline/`.
+ Comparison report under `research/reports/<date>-bls12-results.typ`,
  which quotes predictions from this document and states measured
  values alongside. This document is not edited after commit.

= Explicit non-claims

- No claim that Phase 2 will *deliver* the numbers above. These are
  predictions, not commitments.
- No claim that these are the best achievable numbers on the platform.
  Optimization work (ARMv8-M DSP intrinsics, Hazard3 Zbb / Zba / Zbc
  usage, algorithmic improvements in pairing or MSM) is Phase 3 or
  later.
- No claim that BLS12-381 is a better target than BN254 for embedded
  verification. The choice is driven by ecosystem alignment (Zcash,
  Ethereum sync-committee, Filecoin) rather than raw throughput.
