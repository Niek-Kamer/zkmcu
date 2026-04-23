#import "/research/lib/template.typ": *

// Phase 3.2 results report: STARK Fibonacci verify on RP2350 at
// FieldExtension::Quadratic (~96-bit conjectured security). Cites the
// 2026-04-24-stark-quadratic-prediction.typ numbers verbatim and
// compares them to measurements taken after the prover switch.

#let m33_q   = toml("/benchmarks/runs/2026-04-24-m33-stark-fib-1024-q/result.toml")
#let rv32_q  = toml("/benchmarks/runs/2026-04-24-rv32-stark-fib-1024-q/result.toml")
#let m33_n   = toml("/benchmarks/runs/2026-04-23-m33-stark-fib-1024/result.toml")
#let rv32_n  = toml("/benchmarks/runs/2026-04-23-rv32-stark-fib-1024/result.toml")

#show: paper.with(
  title: "STARK verify at quadratic extension: results on RP2350 M33 + Hazard3",
  authors: ("zkmcu",),
  date: "2026-04-24",
  kind: "report",
  abstract: [
    Phase 3.2 measurements on the RP2350 Pico 2 W. Same AIR and same
    silicon as phase 3.1, prover switched from `FieldExtension::None`
    (63-bit conjectured) to `FieldExtension::Quadratic` (95-bit
    conjectured). Cortex-M33 *70.0 ms*, Hazard3 RV32 *93.0 ms*, total
    RAM on M33 *~100 KB*. The phase-3.1 headline survives: all three
    verifier families (BN254 Groth16, BLS12-381 Groth16, STARK) fit
    under 128 KB SRAM at production-grade security. Four of six M33
    predictions and three of four RV32 predictions fell below their
    predicted bands --- same optimistic-miss pattern as phase 3.1. The
    cross-ISA ratio *narrowed* from 1.46× to 1.33× at Quadratic
    extension, opposite of the prediction; falsification tied to the
    proof being more auth-path-dominated than expected.
  ],
)

= Setup

Identical to phase 3.1 except for three changes, committed as
`phase 3.2.1` (2b6a388):

- `zkmcu-host-gen::stark::run` --- `ProofOptions` field extension
  flipped from `None` to `Quadratic`.
- `zkmcu-verifier-stark::fibonacci::verify` --- `MinConjecturedSecurity`
  raised from 63 to 95 to match.
- Committed vector `crates/zkmcu-vectors/data/stark-fib-1024/proof.bin`
  regenerated. Size went from *25,332 B* to *30,888 B*.

No firmware code change. Both firmware crates consume the bigger
committed proof bytes unchanged.

All other parameters held fixed: 32 queries, blowup 8, grinding 0,
Blake3-256 hash, Goldilocks base field, FRI folding factor 8.

= Headlines

#compare-table(
  ("Quantity", "Phase 3.1 (None, 63-bit)", "Phase 3.2 (Quadratic, 95-bit)", "Δ"),
  (
    ([M33 verify time], [43.8 ms], [70.0 ms], [*+60 %*]),
    ([RV32 verify time], [64.1 ms], [93.0 ms], [+45 %]),
    ([Proof size], [25.3 KB], [30.9 KB], [+22 %]),
    ([M33 heap peak], [70.7 KB], [93.5 KB], [+32 %]),
    ([M33 stack peak], [5.2 KB], [5.6 KB], [+8 %]),
    ([M33 total RAM], [~76 KB], [~100 KB], [+31 %]),
    ([Conjectured security], [63 bits], [95 bits], [+32 bits]),
    ([RV32 / M33 ratio], [1.46×], [1.33×], [*narrower*]),
  ),
)

Cortex-M33 verify at 150 MHz is *70 ms* under production-grade security
at *~100 KB total RAM*. Hazard3 RV32 is *93 ms*.

= The 128 KB tier holds

Phase 3.1's headline was "all three verifier families fit on 128 KB
SRAM". That measurement was at 63-bit conjectured security, which is
below any plausible production threshold. The phase-3.2.0 prediction
report flagged the 128 KB question as *the* most consequential
open variable at Quadratic extension, because a miss in the wrong
direction would break the grant-pitch line.

Measured: *99,655 B ≈ 97.3 KiB on Cortex-M33*. Roughly *31 KB of
margin* under the 128 KB line.

#compare-table(
  ("Verifier family", "Verify (M33, ms)", "Total RAM (M33)", "Fits 128 KB?"),
  (
    ([BN254 Groth16, 1 pub input], [962], [~73 KB], [yes]),
    ([BN254 Semaphore depth 10], [1176], [~74 KB], [yes]),
    ([BLS12-381 Groth16, 1 pub input], [2015], [~88 KB], [yes]),
    ([STARK Fib-1024, 63-bit (phase 3.1)], [43.8], [~76 KB], [yes]),
    ([STARK Fib-1024, 95-bit (phase 3.2)], [*70.0*], [*~100 KB*], [*yes*]),
  ),
)

Headline stands: zkmcu is a family of `no_std` Rust SNARK + STARK
verifiers, all fitting the same hardware-wallet-grade silicon tier at
production-grade security. nRF52832, STM32F405, Ledger ST33, Infineon
SLE78 all remain viable targets.

= Prediction vs measurement

Six M33 quantities and four RV32 quantities were predicted in
`research/reports/2026-04-24-stark-quadratic-prediction.typ` (committed
in e872086, *before* the prover was changed).

== Cortex-M33

#compare-table(
  ("Quantity", "Predicted", "Measured", "Status"),
  (
    ([Verify time], [90--140 ms], [70.0 ms], [*below band*]),
    ([Proof size], [40--50 KB], [30.9 KB], [*below band*]),
    ([Heap peak], [110--140 KB], [93.5 KB], [*below band*]),
    ([Stack peak], [5.2--7 KB], [5.6 KB], [within]),
    ([Total RAM], [115--150 KB], [~100 KB], [*below band*]),
    ([Variance], [0.3--0.8 %], [0.33 %], [within (low end)]),
  ),
)

Four of six below the predicted band. One within (stack), one within
low-end (variance).

== Hazard3 RV32

#compare-table(
  ("Quantity", "Predicted", "Measured", "Status"),
  (
    ([Verify time], [130--210 ms], [93.0 ms], [*below band*]),
    ([Stack peak], [5.2--7 KB], [5.5 KB], [within]),
    ([Variance], [0.5--1.2 %], [0.29 %], [*below band*]),
    ([Cross-ISA ratio], [1.45--1.65×], [1.33×], [*below band*]),
  ),
)

Three of four below the predicted band.

== Combined

*Seven falsification criteria fired across the two targets.* All in
the optimistic direction --- same pattern as phase 3.1, which also had
three of four predictions fall below band.

= Why the model was pessimistic, again

The phase 3.2.0 cost model assumed three things that compounded into
a too-pessimistic prediction band:

/ Extension-field multiplications are ~3× base-field cost: correct as
  a point estimate, but the band was hedged to 2.1--3.2× against the
  possibility that winterfell uses schoolbook ($4 times$ + reduction)
  or that reduction dominates. Neither materialised.

/ FRI layer evaluations dominate proof size: wrong. Proofs are
  auth-path-dominated --- 32 queries × ~13 layers × 32 B/hash is
  ~13 KB of auth-path hashes that don't change under Quadratic.
  The FRI evaluations doubling only pushes the total from 25.3 KB to
  30.9 KB (+22 %), not the 70 %+ the model predicted.

/ Quadratic amplifies M33's register / UMULL advantage: also wrong.
  The larger proof shifts the workload mix *toward* hash work (which
  scales more linearly across the two ISAs) and *away from* the pure
  64-bit-mul path. Cross-ISA ratio narrowed rather than widened.

All three have a common root: *the proof isn't FRI-fold-heavy enough
for the "amplify field work" model to apply cleanly*. For a larger AIR
or a circuit with more complex transition constraints, the balance
shifts and the model's assumptions may start to hold. Phase 4 territory.

= Variance

Phase 3.1 flagged the variance anomaly (0.30 % M33, 0.69 % RV32 vs
predicted 0.03--0.1 %) and hypothesised it came from `proof.clone()`
inside the timed window. The clone allocates buffers; the allocator
path introduces jitter.

Phase 3.2 predicted the anomaly would get slightly worse under
Quadratic (bigger proof → bigger clone → more jitter). It didn't.

- M33: *0.30 % → 0.33 %*. Essentially unchanged.
- RV32: *0.69 % → 0.29 %*. Tighter.

This weakens the `proof.clone()` hypothesis. Plausible alternative:
allocator jitter is fixed-cost per allocation *call* rather than
per-byte, so the same number of `Vec::with_capacity` calls produces
similar absolute jitter in both configurations. Over a longer
measurement window (93 ms vs 64 ms on RV32), the fractional variance
drops even though absolute jitter is unchanged.

Not a blocker. Worth pinning down in phase 3.2.x by moving the clone
outside the timed window explicitly.

= Cross-curve comparison on same silicon

All measured on the Raspberry Pi Pico 2 W Cortex-M33 at 150 MHz:

#compare-table(
  ("Circuit", "Verify (M33)", "Ratio to STARK (Q)"),
  (
    ([STARK Fib-1024, 95-bit *(phase 3.2)*], [70.0 ms], [1.0×]),
    ([STARK Fib-1024, 63-bit (phase 3.1)], [43.8 ms], [0.63×]),
    ([BN254 Groth16, 1 pub input], [962 ms], [14× slower]),
    ([BN254 Semaphore depth 10], [1,176 ms], [17× slower]),
    ([BLS12-381 Groth16, 1 pub input], [2,015 ms], [29× slower]),
  ),
)

Even at production-grade 95-bit security, the STARK verify is *14–29×
faster* than any Groth16 verify on this hardware. The tradeoff
remains proof size:

#compare-table(
  ("Proof system", "Wire proof size"),
  (
    ([BN254 Groth16], [256 B (constant)]),
    ([BLS12-381 Groth16], [512 B (constant)]),
    ([Semaphore Groth16], [256 B (BN254 underneath)]),
    ([STARK Fib-1024, 63-bit], [25.3 KB]),
    ([STARK Fib-1024, 95-bit *(phase 3.2)*], [*30.9 KB*]),
  ),
)

Classic throughput-for-bandwidth tradeoff. For receiver-bound
transports (LoRa, NFC, low-bandwidth BLE), Groth16 wins on wire
size; for verify-bound devices on fat transports (USB, Wi-Fi), STARK
wins on verify time.

= Findings

+ *The 128 KB SRAM tier holds at production-grade STARK security.*
  ~100 KB total RAM, 31 KB margin. Phase 3.1 headline survives.

+ *Field-extension cost is less severe than predicted.* Verify went
  up 60 % on M33, not the 110--220 % the model expected. Proof size
  went up 22 %, not 70 %.

+ *The cross-ISA ratio narrows under Quadratic.* M33 advantage
  compresses from 1.46× to 1.33× because the larger proof shifts the
  workload mix toward hash work. Falsifies the "more field arithmetic
  → M33 pulls ahead" hypothesis.

+ *Variance anomaly does not scale with proof size.* Strongly
  suggests `proof.clone()` allocation count, not allocation bytes,
  drives the jitter. Worth a dedicated phase-3.2.x experiment.

+ *Our prediction model remains systematically pessimistic.* Phase
  2.6 BLS12-381: 4 of 7 criteria fired below band. Phase 3.1 STARK
  None: 3 of 4 fired below band. Phase 3.2 STARK Quadratic: 7 of 10
  fired below band. All three in the optimistic direction. The
  discipline of writing the prediction before measurement still
  pays --- it's the only way to know *how* the model is wrong, which
  drives the next iteration --- but the prior for future bands
  should shift toward the cheaper end.

= Limitations

- 95-bit conjectured, not proven. Same winterfell-default caveat as
  phase 3.1.
- RV32 `heap_peak` still not captured (plain `LlffHeap`, no
  `TrackingHeap` port). Phase-3.2.x cleanup.
- `.text` / `.data` / `.bss` not extracted. Same limitation as all
  earlier phase runs.
- `proof.clone()` still inside the timed window. Orthogonal
  phase-3.2.x experiment will isolate its jitter contribution.
- Single AIR, single trace length. Fibonacci at $N = 1024$ is the
  STARK hello-world, not a representative production workload.
  Phase-4 candidate: bigger AIR or Miden VM trace verify.

= Open questions for phase 3.3 and beyond

- What does verify time look like for a larger AIR (more transition
  constraints, higher-degree constraints)? The proof becomes less
  auth-path-dominated as column count + constraint complexity grow,
  so the cross-ISA and field-extension sensitivities may look
  different.
- Does the cross-ISA narrowing hold at $N = 2^(16)$ or $N = 2^(18)$,
  or does it reverse direction once the FRI layer count grows?
- Would `BabyBear` or `Mersenne-31` beat Goldilocks on 32-bit MCU
  inner loops? Winterfell supports custom fields; this is mostly a
  porting exercise.
- With grinding lifted to a non-zero value (say 20 bits), conjectured
  security reaches ~115 bits with near-zero verify cost. Worth
  measuring as a separate phase-3.2.x sub-experiment.

#v(1em)

_Prediction report_: `research/reports/2026-04-24-stark-quadratic-prediction.typ`.

_Phase 3.1 results report_: `research/reports/2026-04-23-stark-results.typ`.

_Phase 3.1 prediction report_: `research/reports/2026-04-23-stark-prediction.typ`.
