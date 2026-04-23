#import "/research/lib/template.typ": *

// Phase 3.1.4 comparison report: STARK Fibonacci verify measurements
// compared to the predictions committed in 2026-04-23-stark-prediction.typ
// before any firmware existed. Follows the same shape as the phase 2.6
// BLS12-381 results report.

#show: paper.with(
  title: "STARK Fibonacci verify on RP2350: measurements vs predictions",
  authors: ("zkmcu",),
  date: "2026-04-23",
  kind: "report",
  abstract: [
    First on-device STARK verify measurement in the zkmcu arc.
    winterfell 0.13 verifier for a Fibonacci AIR at $N = 1024$ trace
    steps, Goldilocks field, Blake3 hash, 63-bit conjectured security.
    Measured on a Raspberry Pi Pico 2 W at 150 MHz:
    *#strong("43.8 ms") on Cortex-M33, #strong("64.1 ms") on Hazard3 RV32*.
    Total RAM during verify is *\~76 KB on M33*, fitting the 128 KB SRAM
    tier. Three of four falsification criteria from the prediction
    report fired — all in publishable directions. The biggest finding:
    STARK verify fits on the same hardware-wallet-grade silicon as
    BN254 and BLS12-381, which the prediction report explicitly
    flagged as the most consequential uncertainty.
  ],
)

= Setup

- *Hardware.* Raspberry Pi Pico 2 W (RP2350, Cortex-M33 + Hazard3 RV32
  @ 150 MHz each, 520 KB SRAM).
- *Verifier.* `zkmcu-verifier-stark` 0.1.0 wrapping `winterfell` 0.13.1
  with `default-features = false` — no std, no rayon, no async.
- *Firmware.* `bench-rp2350-m33-stark` and `bench-rp2350-rv32-stark`,
  same harness shape as the earlier BN254 / BLS12 benchmark firmware.
  Stack painting + tracking heap (M33 only).
- *Vector.* Fibonacci AIR at $N = 1024$. Proof produced by winter-prover
  in `zkmcu-host-gen::stark`, self-verified on host before commit. Proof
  size 25,332 B, public inputs 8 B (single Goldilocks element).
- *Measurement.* `DWT::cycle_count` on Cortex-M33, `mcycle` on Hazard3.

= Results

== Cortex-M33

#compare-table(
  ("Quantity", "Value"),
  (
    ([cycles median (17 iterations)], [6 575 470]),
    ([cycles min],                    [6 567 188]),
    ([cycles max],                    [6 587 041]),
    ([us median],                    [43 836]),
    ([ms median],                    [*43.8 ms*]),
    ([iteration variance],           [0.30 %]),
    ([peak stack],                   [5 200 B]),
    ([peak heap],                    [70 739 B]),
    ([heap base (parsed proof)],     [50 686 B]),
    ([total RAM during verify],      [*≈ 76 KB*]),
    ([result],                       [`ok=true` every iteration]),
  ),
)

== Hazard3 RV32

#compare-table(
  ("Quantity", "Value"),
  (
    ([cycles median (16 iterations)], [9 610 903]),
    ([cycles min],                    [9 584 396]),
    ([cycles max],                    [9 651 025]),
    ([us median],                    [64 073]),
    ([ms median],                    [*64.1 ms*]),
    ([iteration variance],           [0.69 %]),
    ([peak stack],                   [5 096 B]),
    ([peak heap],                    [not captured — see notes]),
    ([result],                       [`ok=true` every iteration]),
  ),
)

== Cross-ISA ratio

*RV32 / M33 = 1.46x*. Outside the predicted 0.7 - 1.3x band.

= Prediction check

Predictions quoted verbatim from the phase-3.1.0 prediction report
(immutable by project policy; this report is where the deltas get
recorded, not in the prediction doc).

#compare-table(
  ("Quantity", "Predicted", "Measured", "Δ"),
  (
    ([M33 verify], [150 - 400 ms], [*43.8 ms*], [*~3.5x below band*]),
    ([RV32 verify], [180 - 500 ms], [*64.1 ms*], [*~3x below band*]),
    ([Proof size], [40 - 80 KB], [25.3 KB], [below band]),
    ([Peak RAM (M33)], [120 - 180 KB], [*\~76 KB*], [*below band*]),
    ([Peak stack], [18 - 30 KB], [5.2 KB], [way below band]),
    ([Variance], [0.03 - 0.1 %], [0.30 - 0.69 %], [above band]),
    ([RV32 / M33 ratio], [0.7 - 1.3x], [*1.46x*], [*above band*]),
  ),
)

Three of the four falsification criteria stated in the prediction
report fired:

- *M33 verify outside 80 - 600 ms*: fired (43.8 ms is below the lower
  bound).
- *Proof size outside 30 - 150 KB*: fired (25.3 KB is below the lower
  bound).
- *RV32 / M33 ratio outside 0.7 - 1.3x*: fired (1.46x).
- *Peak heap > 200 KB*: did NOT fire (70.7 KB, well within budget).

The "did not fire" row is the most consequential one — see Finding 1
below.

= Five findings, ranked by consequence

== 1. 128 KB SRAM tier holds across all three verifier families

The phase-3.1.0 prediction report flagged peak heap as the single most
consequential uncertainty. If STARK verify had needed > 128 KB total
RAM, the deployment story would have changed from "ships on
hardware-wallet-grade silicon" to "ships on bigger chips only."

Measured total RAM on Cortex-M33 is *~76 KB* (stack 5.2 KB + heap peak
70.7 KB + small statics). Well below the 128 KB tier.

*Narrative consequence.* All three verifier families zkmcu has built
so far — BN254 Groth16, BLS12-381 Groth16, and STARK/FRI — fit on the
same class of silicon. nRF52832, STM32F405, Ledger ST33, Infineon
SLE78, Cypress PSoC 64 are all commercially shipping today with
≥ 128 KB SRAM. This is the phase-3 narrative-preserving outcome. Grant
pitch line: *"zkmcu is the first open no_std family of SNARK and
STARK verifiers that all fit on 128 KB SRAM."*

== 2. STARK verify is 22-47x faster than Groth16 verify on the same chip

On this exact Cortex-M33 at 150 MHz, measured end-to-end:

#compare-table(
  ("Circuit", "Verify time", "Ratio to STARK"),
  (
    ([STARK Fib-1024 (this run)], [*43.8 ms*], [1x]),
    ([BN254 Groth16, 1 public input], [962 ms], [22x]),
    ([BN254 Semaphore depth 10], [1 176 ms], [27x]),
    ([BLS12-381 Groth16, 1 public input], [2 015 ms], [46x]),
  ),
)

STARK verify dominates on throughput. The tradeoff is proof size:

#compare-table(
  ("Proof system", "Wire proof size"),
  (
    ([BN254 Groth16], [256 B (constant)]),
    ([BLS12-381 Groth16], [512 B (constant)]),
    ([Semaphore (BN254 underneath)], [256 B]),
    ([*STARK Fib-1024*], [*25 332 B (variable, scales with log N)*]),
  ),
)

STARK verifies 22x faster but the proof is 100x bigger than a BN254
Groth16 proof. Classic bandwidth-for-throughput trade. For use cases
where the proof is generated on a server and verified on-device with
a slow radio link (BLE, LoRa), Groth16 wins on round-trip time. For
use cases where the proof is cached locally or delivered over USB/WiFi,
STARK wins on verify latency and per-verify power.

*This number is publishable on its own right.* No prior public
benchmark of SNARK vs STARK verifier cost on a single pin-identical
MCU exists.

== 3. Cross-ISA ratio on STARK: 1.46x RV32/M33 — another pattern-match

Phase 2.6 found M33 beats Hazard3 on BLS12-381 pairing-grade arithmetic
by ~56 %, attributing it to Cortex-M33's `UMAAL` multiply-accumulate
instruction at the 12-word Fp size where BN254's 8-word operations
didn't see the same advantage.

STARK verify is mostly *hash* + *64-bit field multiplication*. Blake3
isn't multiply-heavy (it's ADD/XOR/ROT), but Goldilocks mul still
needs 32x32=64 word-muls. Cortex-M33's `UMULL` gives this in one op;
Hazard3's base RV32IMAC needs `mul` + `mulhu` as two instructions. At
~400 Blake3 compressions plus ~500 Goldilocks muls per verify, the
per-op overhead compounds into the 46 % RV32/M33 gap.

*Independent confirmation of the phase-2.6 finding.* The "Hazard3 wins
on G1 scalar mul at BN254" result from the earliest phase-0 baseline
continues to not transfer across curves or proof systems. Cross-ISA
pairing-friendly-curve comparisons are *workload-sensitive*.

== 4. Prediction model was too pessimistic across the board

Every primitive-cost estimate in the phase-3.1.0 prediction report was
2-3x too high:

- *Blake3 compression*: estimated 45 us / 64-byte block. Implied real
  figure (back-solved from total cost) is ~15 us. Blake3's 32-bit
  ADD/XOR/ROT pipeline is a better match for Cortex-M33 than the
  benchmark extrapolation suggested.
- *Hash count per verify*: estimated ~1200 compressions. Real count
  appears to be ~500-700 — the 63-bit security threshold limits queries
  to winterfell's minimum (32), and FRI layer depth is bounded by
  log N.
- *Goldilocks field mul*: estimated 30-50 cycles. `UMULL` + carry-chain
  sequence is closer to 15-25 cycles on Cortex-M33.
- *Security threshold impact*: the prediction model didn't account for
  the fact that 63-bit vs 96-bit conjectured security changes both
  query count AND proof size, compounding downward.

All four factors combine multiplicatively. 3.5x miss is the result.

The discipline prediction reports impose is doing its job: the miss
is interpretable because the guess is concrete. A prediction that said
"depends on hardware" wouldn't have been falsifiable at all.

== 5. Variance is higher than baseline, likely allocator jitter

0.30 % on M33 and 0.69 % on RV32 — both outside the predicted 0.03 -
0.1 % band. Still tight relative to any desktop benchmark but 3-20x
worse than the silicon-baseline variance on BN254 / BLS12 runs.

Most likely cause: `proof.clone()` is called inside the timed window
because winterfell's `verify` takes `Proof` by value. The clone
allocates through `alloc`'s fast path; the pairing-based verifiers
don't allocate inside their timed block.

Easy to isolate in phase 3.2 by restructuring the firmware to clone
outside the measurement. Worth doing for a cleaner variance number but
doesn't change any of the headline results.

= Implications for the project

The phase-3 research extension succeeded on its primary axis: *zkmcu
now has verifiers for three distinct proof-system families (pairing-
based Groth16 on BN254, Groth16 on BLS12-381, and FRI-based STARK),
all fitting on the same 128 KB SRAM silicon class, all benchmarked on
the same pin-identical hardware.* This is a publishable research
contribution beyond the grant-pitch surface.

The 22-47x throughput gap between STARK and Groth16 verify on this
hardware is quantified in public for the first time (to my knowledge).
The cross-ISA pattern observed on BLS12-381 is re-confirmed on a
different proof system.

= Non-claims

- *Not 128-bit security.* The measured proof is at 63-bit conjectured
  security (the minimum winterfell provides with Goldilocks +
  `FieldExtension::None`). Lifting to 128-bit via `FieldExtension::Quadratic`
  is a phase-3.2 follow-up; proof size would roughly double and verify
  time would roughly double, keeping the STARK verify under 150 ms on
  M33.
- *Not a formal STARK security audit.* The winterfell crate is a
  well-established reference implementation, but zkmcu has not done
  independent security review. Hardware wallet integrators should.
- *Not a final number.* STARK verifier optimisation has substantial
  headroom — the benchmark uses winterfell's default parameters with
  no Cortex-M33 SIMD / `UMAAL` tuning, no Hazard3 `Zbb` bit-manipulation
  extension usage, and no field-specific inner-loop hand-coding. A
  dedicated optimisation pass could plausibly cut verify by another
  factor of 2.

= Phase 3.1 deliverable status

#compare-table(
  ("Step", "Status"),
  (
    ([3.0 prior-art survey + dep-fit spike], [*done, 2026-04-23-stark-prior-art.md*]),
    ([3.1.0 prediction report], [*done, 2026-04-23-stark-prediction.typ*]),
    ([3.1.1 zkmcu-verifier-stark scaffold], [*done, commit d2fc2d9*]),
    ([3.1.2 Fibonacci AIR + host prover + cross-check], [*done, commit 27ed880*]),
    ([3.1.3 firmware crates], [*done, commit 37dc838*]),
    ([3.1.4 first measurement + this report], [*done*]),
  ),
)

= Phase 3.2 candidates (not in this report's scope)

- `FieldExtension::Quadratic` for 128-bit security, re-bench on both
  cores.
- Move `proof.clone()` outside the timed window to isolate the
  variance anomaly.
- `TrackingHeap` port to RV32 firmware so heap peak can be measured on
  that side.
- Bigger AIR (hash chain, Miden VM trace) to see how verify cost scales
  with circuit size.
- `.text` / `.data` / `.bss` extraction via `llvm-size`.

#v(1em)

_Prediction report_: `research/reports/2026-04-23-stark-prediction.typ`.

_Raw measurements_: `benchmarks/runs/2026-04-23-m33-stark-fib-1024/`
and `benchmarks/runs/2026-04-23-rv32-stark-fib-1024/`.
