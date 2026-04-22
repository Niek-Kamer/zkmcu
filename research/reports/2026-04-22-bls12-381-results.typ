#import "/research/lib/template.typ": *

// Companion to 2026-04-22-bls12-381-prediction.typ. Reads the committed
// benchmark TOMLs and states measured values next to the predictions
// from the prediction report (which is itself immutable and not edited
// even if wrong — corrections happen in this report).

#let m33 = toml("/benchmarks/runs/2026-04-22-m33-bls12-baseline/result.toml")
#let rv32 = toml("/benchmarks/runs/2026-04-22-rv32-bls12-baseline/result.toml")

#show: paper.with(
  title: "BLS12-381 Groth16 on RP2350: measurements vs predictions",
  authors: ("zkmcu",),
  date: "2026-04-22",
  kind: "report",
  abstract: [
    First on-device measurements of BLS12-381 Groth16 verify on the
    RP2350 at 150 MHz. Cortex-M33 verify is *#{m33.bench.groth16_verify_square.us_median / 1000} ms*
    (1 public input), Hazard3 RV32 is *#{rv32.bench.groth16_verify_square.us_median / 1000} ms*,
    iteration-to-iteration variance under 0.07 % on both cores, every
    iteration returns `ok=true`. Four predictions from the pre-measurement
    report missed outside their stated error bounds, in different
    directions — the most consequential (peak heap, driving SRAM-tier
    alignment) was off by −40 %, which preserves the 128 KB SRAM tier
    narrative. The secondary finding is that the BN254 "Hazard3 beats
    Cortex-M33 on G1 scalar mul" result *does not generalize* to
    BLS12-381: on BLS12 Hazard3 loses on every primitive. Cross-ISA
    comparisons on pairing-friendly curves are prime-size-dependent, not
    algorithm-dependent.
  ],
)

= Setup

Measured on a Raspberry Pi Pico 2 W (RP2350) at 150 MHz, `rustc 1.94.1`,
`release` profile with `lto = "fat"`, `opt-level = "s"`,
`codegen-units = 1`. Crypto backend on-device is zkcrypto `bls12_381`
0.8 with `default-features = false, features = ["groups", "pairings",
"alloc"]`; host-side proof generator is `ark-bls12-381` 0.5 +
`ark-groth16` 0.5. Wire format is EIP-2537. Vectors at
`crates/zkmcu-vectors/data/bls12-381/{square,squares-5}`. Firmware
arena is 256 KB on both cores (oversized for first bring-up); heap peak
on M33 is measured via a `TrackingHeap` GlobalAlloc wrapper.

= Headline: predictions vs measurements

Predictions quoted verbatim from
`research/reports/2026-04-22-bls12-381-prediction.typ` (not edited
after commit, by project policy).

#compare-table(
  ("Operation", "M33 predicted", "M33 measured", "Δ", "RV32 predicted", "RV32 measured", "Δ"),
  (
    ([G1 scalar mul (typical)], [\~275 ms], [*847 ms*], [*+208 %*], [\~180 ms], [*1 427 ms*], [*+693 %*]),
    ([G2 scalar mul (typical)], [\~525 ms], [523 ms], [0 %], [—], [1 003 ms], [—]),
    ([Pairing], [\~1 220 ms], [*607 ms*], [*−50 %*], [\~1 620 ms], [1 975 ms], [+22 %]),
    ([Verify (1 pub)], [2 100–2 400 ms], [*2 015 ms*], [below low end], [2 800–3 200 ms], [*5 151 ms*], [*+61 % outside band*]),
    ([Verify (5 pub)], [—], [5 408 ms], [—], [—], [10 870 ms], [—]),
    ([Peak heap], [\~130 KB], [*77.5 KB*], [*−40 %*], [—], [not measured], [—]),
    ([Peak stack], [\~18 KB], [19.4 KB], [+8 %], [—], [20.6 KB], [—]),
    ([Total RAM in verify], [\~180 KB], [*\~97 KB*], [*−46 %*], [—], [—], [—]),
  ),
)

Five of the seven directly-comparable M33 predictions missed outside
their stated error bounds. Four of them in different directions —
*G1 slower than predicted* (×3), *pairing faster than predicted* (½×),
*peak heap lower than predicted* (60 %), *verify just under the
prediction band*. G2 scalar mul came in almost exact.

= Five findings, ranked by consequence

== 1. The 128 KB SRAM tier story survives (peak-heap undershoot)

The prediction flagged peak heap as the single most consequential
uncertainty: if measured heap exceeded 110 KB, the verifier would no
longer fit on the 128 KB SRAM-tier silicon that the BN254 narrative
depended on (nRF52832, STM32F405, Ledger ST33, Infineon SLE78,
similar hardware-wallet-grade parts), and the grant pitch would need
a significant rework.

Measured peak heap on M33: *#{m33.footprint.heap_peak_bytes} B*
(≈ 77.5 KB) — *below* the BN254 peak of 81.3 KB. Stack peak is
slightly higher (19.4 KB vs 15.6 KB for BN254), so total RAM
during verify is ~97 KB, essentially unchanged. The #{m33.bench.groth16_verify_square.public_inputs}-input and
#{m33.bench.groth16_verify_squares_5.public_inputs}-input circuits report identical heap peaks, confirming the
`pairing_batch` workspace dominates heap allocation the same way it
does for BN254.

The mechanism: zkcrypto's pairing uses stack-allocated `G2Prepared`
structures for Miller-loop line coefficients where substrate-bn
heap-allocates an Fq12 polynomial workspace. The same allocations
show up in a different memory region, but the aggregate is
smaller, not larger.

Consequence: BLS12-381 verify fits on the same MCU class as the
BN254 verifier. No narrative pivot needed. The `research/reports/
2026-04-22-bls12-381-prediction.typ` falsification criterion for
peak heap (`"100 – 200 KB"`) fired on the low side — publishable, not
catastrophic.

== 2. BN254 → BLS12-381 does not transfer the RISC-V G1 advantage

The BN254 baseline showed Hazard3 running G1 scalar multiplication
35 % *faster* than Cortex-M33 on identical silicon (110 ms vs 72 ms).
The `research/prior-art/main.typ` survey highlighted this as the
first publicly reported cross-ISA pairing-grade arithmetic comparison
at this MCU scale, and framed it as "RISC-V's 31 GP registers reduce
spill traffic during schoolbook Fp multiplication."

On BLS12-381 the sign of the difference is reversed: Hazard3 is
*69 % slower* than Cortex-M33 on G1 scalar mul (1 427 ms vs 847 ms).
The G2, pairing, and full-verify gaps all widen compared to BN254 —
the RV32 / M33 ratio on full verify goes from 1.39× (BN254) to 2.56×
(BLS12-381).

Mechanism (to be confirmed with a disassembly dive): Cortex-M33's
`UMAAL` instruction performs a 32 × 32 → 64-bit multiply plus a
two-accumulator add in one instruction. Base RV32IMAC has `mul` and
`mulhu` as separate instructions with no accumulate form. On BN254's
8-word Fp, schoolbook multiplication produces 64 word-mul ops, and
the per-op overhead of two separate RV32 instructions plus a carry-
chain sequence does not dominate. On BLS12-381's 12-word Fp
(2.25× more word-muls), that overhead compounds, and the register
advantage no longer offsets it.

Consequence: *cross-ISA comparisons on pairing-friendly curves are
prime-size-dependent, not just algorithm-dependent*. Drawing
conclusions from BN254 on a 8-word prime and transferring them to
BLS12-381 on a 12-word prime is invalid without re-measurement. This
is the main empirical contribution of this phase.

== 3. Pairing is half the predicted cost

The prediction multiplied BN254's 533 ms pairing by 2.3× (a rough
field-size-scaling factor) and got \~1 220 ms. The measured BLS12-381
pairing on M33 is *#{m33.bench.pairing.us_median / 1000} ms* — only
14 % slower than BN254, not 130 % slower.

The miss is structural. BLS12-381's ate pairing has a shorter Miller
loop than BN254: the curve parameter `t` has 6 non-zero bits
(`0xd201000000010000`) against BN254's `u` with 27 non-zero bits
(`0x44e992b44a6909f1`). The shorter loop offsets the larger per-op
cost. The prediction did not model Miller-loop length as a first-class
variable.

Consequence: on M33, pairing is competitive across the two curves
at the field-arithmetic scale we're at. On RV32 the savings are
almost entirely consumed by the field-mul penalty (see finding 2), so
the pairing ratio RV32/M33 is 3.26× there vs 1.14× on M33.

== 4. G1 scalar mul is constant-time-shaped on zkcrypto

On BN254, `substrate-bn`'s G1 scalar mul showed a 2× spread across
iterations due to scalar-Hamming-weight-dependent NAF window paths —
30 ms best, 59 ms typical, 110 ms median-over-many-iterations.

On BLS12-381, `bls12_381`'s G1 scalar mul reports a
#{m33.bench.g1_mul.cycles_max - m33.bench.g1_mul.cycles_min}-cycle spread over 7 iterations with random scalars — 0.09 %
of the median. Distribution is unimodal.

This suggests zkcrypto uses a scalar-oblivious implementation
(likely an always-add double-and-add ladder, or a windowed variant
where every window is processed uniformly). *Not a formal
constant-time claim* — CT needs dudect / ChipWhisperer-class
validation — but a useful informal security-positive. The
`SECURITY.md` threat model explicitly marks CT as not-validated;
this observation does not change that, but it does note that a
trivial timing-leak via scalar Hamming weight is (informally)
absent on BLS12-381, unlike BN254.

This also explains why G1 scalar mul on BLS12-381 is proportionally
more expensive than pure field-size scaling predicts: substrate-bn's
scalar-dependent shortcuts are not available.

== 5. Each extra public input costs exactly one G1 scalar mul

On both cores, the delta between the 1-input and 5-input verify is
within 0.2 % of four times the G1 scalar mul time:

#compare-table(
  ("Core", "G1 mul", "Δ per extra input", "Match"),
  (
    ([M33], [#{m33.bench.g1_mul.us_median / 1000} ms], [#{m33.bench.scaling.us_per_extra_input / 1000} ms], [within 0.2 %]),
    ([RV32], [#{rv32.bench.g1_mul.us_median / 1000} ms], [#{rv32.bench.scaling.us_per_extra_input / 1000} ms], [within 0.2 %]),
  ),
)

Consequence for circuit designers: on BLS12-381 verify on the M33 tier,
each additional public input adds \~847 ms of on-device verify cost,
substantial next to the 2 015 ms fixed base. Circuits targeting
embedded verifiers should minimise public-input count aggressively
(fold public state into a single hash-commitment Fr input, etc.).
Contrast with BN254 where small-integer public inputs cost only
~3 ms each — the BLS12 scalar-mul constant-time-ness raises the
per-input floor substantially.

= Falsification check

The prediction report's falsification criteria:

- Full verify on M33 outside *1.5 – 3.5 s*: measured 2.015 s. *Within
  band* (on the low side).
- Hazard3 RV32/M33 ratio within ±20 % of 1.33× predicted: measured
  2.56×. *Outside band* (+92 % over predicted).
- Peak heap outside *100 – 200 KB*: measured 77.5 KB. *Outside band*
  on the low side.

Two of three falsification criteria fired, both in publishable
directions. The explicit pre-measurement statement of these
criteria is what made this result interpretable rather than anecdotal.

= Updated project-level narrative

The BN254 story was _first no_std Groth16 SNARK verifier for ARM
Cortex-M, \~1 s verify, 128 KB SRAM tier_. After this report the
accurate narrative is:

#block(inset: (x: 1em, y: 0.5em), width: 100%, fill: luma(248))[
  zkmcu is an open, reproducibly-benchmarked family of `no_std` SNARK
  verifiers for microcontrollers. Groth16/BN254 and Groth16/BLS12-381
  are both supported, both benchmarked on the same silicon, both
  memory-tier-compatible with commodity hardware-wallet-class MCUs.
  On Cortex-M33 @ 150 MHz: BN254 verify in 962 ms, BLS12-381 in 2 015 ms.
  On Hazard3 RV32 @ 150 MHz: BN254 verify in 1 341 ms, BLS12-381 in
  5 151 ms. Total RAM during verify is preserved across curves at
  ~97 KB, keeping the 128 KB SRAM tier reachable on both.
]

Two results from this phase that are publishable in their own right:

+ *BLS12-381 BN254 / zkcrypto substrate-bn cross-ISA transferability.*
  The register-count advantage of Hazard3 on BN254 G1 scalar mul does
  not generalise to BLS12-381; the sign of the M33/RV32 ratio
  reverses. Empirically motivates caution when drawing cross-curve
  conclusions from single-curve benchmarks.
+ *zkcrypto BLS12-381 G1 scalar mul is effectively time-invariant
  with respect to scalar Hamming weight at cycle-count resolution*
  over 7 random scalars. Not a CT proof; a useful informal positive.

= Non-claims

- No claim that these numbers are close to what an optimised
  implementation can achieve. `bls12_381`'s G1 scalar mul at 847 ms
  on M33 (vs `substrate-bn`'s 110 ms on BN254 via a window-NAF) is a
  clear optimisation target. At minimum ~4× headroom, probably more
  once ARMv8-M DSP intrinsics are brought to bear.
- No claim that BLS12-381 is a worse target than BN254 for embedded
  verification. The choice is ecosystem-driven (Zcash, Ethereum sync-
  committee, Filecoin), not performance-driven.
- No claim that the prediction report's methodology was correct. Two
  of three falsification criteria fired. A better prediction model
  would treat Miller-loop length, `UMAAL`-class multiply-accumulate
  availability, and scalar-mul window strategy as first-class
  variables, not aggregate them into a single field-size ratio.

= Follow-ups

- Heap-tuned M33 BLS12 rebuild at 96 KB arena, matching the BN254
  `heap-96k-confirmed` methodology. Predict: runs clean. Commit as
  `benchmarks/runs/<date>-m33-bls12-heap-96k-confirmed/`.
- TrackingHeap port to the RV32 firmware to capture heap peak on
  Hazard3. Predict: ~79 KB, same as M33 (allocations come from the
  same crate).
- Disassembly diff between M33 and RV32 for one `bls12_381::Fp`
  multiplication. Goal: confirm the `UMAAL` vs `mul/mulhu/adc`
  hypothesis for finding 2.
- Stripped-ELF `.text` / `.data` / `.bss` extraction. Needs
  `arm-none-eabi-size` / `riscv32-unknown-elf-size` in CI.

#v(1em)

_Prediction report:_ `research/reports/2026-04-22-bls12-381-prediction.typ`.

_Raw measurements:_ `benchmarks/runs/2026-04-22-m33-bls12-baseline/`
and `benchmarks/runs/2026-04-22-rv32-bls12-baseline/`.
