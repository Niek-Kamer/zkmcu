#import "/research/lib/template.typ": *

// Prediction report for phase 3.1: first STARK verifier benchmark on MCU.
// Published before any zkmcu-verifier-stark crate exists so the predictions
// remain falsifiable. Same discipline as the BLS12-381 prediction report at
// 2026-04-22-bls12-381-prediction.typ.

#show: paper.with(
  title: "STARK verify on RP2350: predicted performance (pre-measurement)",
  authors: ("zkmcu",),
  date: "2026-04-23",
  kind: "report",
  abstract: [
    Numerical predictions for the first STARK verifier on microcontrollers
    in the zkmcu arc. Target: a winterfell 0.13 verifier for a Fibonacci
    AIR at $N = 1024$ trace steps, 96-bit security, Goldilocks field,
    Blake3 hash, running on the RP2350 Cortex-M33 at 150 MHz. Predicted
    verify time *150--400 ms*, predicted proof size *~50 KB*, predicted
    peak RAM *100--200 KB*. Confidence is low — never measured a STARK
    verifier on this silicon tier, and the scaling model is adapted from
    desktop winterfell runs. Published before any STARK firmware exists
    so the delta is interpretable as a scientific result, not a narrative
    exercise.
  ],
)

= Why this document exists

Phase 2's BLS12-381 prediction report demonstrated the falsification
criterion pattern: two of three criteria fired, both in publishable
directions, and the committed predictions made the difference
interpretable. Phase 3.1 extends the same discipline to the first
STARK verifier measurement. This document goes into git before any
`zkmcu-verifier-stark` code, any host-side Fibonacci prover, or any
firmware is written. A later comparison report will quote these numbers
verbatim next to the measured values. This document is not edited
after commit.

= Setup assumptions

== Target stack

- *Hardware.* Raspberry Pi Pico 2 W (RP2350, ARM Cortex-M33 @ 150 MHz
  and Hazard3 RV32 @ 150 MHz; 520 KB SRAM).
- *Toolchain.* `rustc 1.94.1`, `release` profile with `lto = "fat"`,
  `opt-level = "s"`, `codegen-units = 1`, `panic = "abort"`.
- *Verifier crate.* `winterfell = "0.13"` with `default-features = false`.
  Dep-fit confirmed in `research/notebook/2026-04-23-stark-prior-art.md`;
  both `thumbv8m.main-none-eabihf` and `riscv32imac-unknown-none-elf`
  build clean with zero warnings and no patches.
- *Hash.* `winter-crypto`'s Blake3-based Merkle hasher. Blake3 falls back
  to pure-Rust on both embedded targets (no C / SIMD cross-compile).
- *Field.* Goldilocks ($p = 2^64 - 2^32 + 1$). 64-bit prime. Winterfell's
  default; documented inner-loop performance on 32-bit hardware comes at
  a 2--4× cost over BabyBear, but Goldilocks is what the winterfell
  reference benchmarks use.
- *AIR.* Fibonacci at $N = 1024$ trace steps with blowup factor 8
  (LDE domain $= 8192$). 2-column trace, trivial constraint
  ($a_(k+2) = a_k + a_(k+1)$). Smallest meaningful winterfell circuit.
- *Security.* 96-bit conjectured STARK security. Winterfell default for
  the Fibonacci example.

== Measurement methodology

Same as phases 2.1--2.7: `DWT::cycle_count` on Cortex-M33, `mcycle` on
Hazard3, at least 7 iterations per datapoint, USB-CDC serial output,
stack painting + `TrackingHeap` on the M33 firmware.

= Predictions

== Proof size

Winterfell's Fibonacci example at default parameters commits:

- Trace LDE polynomials (2 columns) → Merkle tree over the LDE domain
- Constraint composition polynomial quotients → another Merkle tree
- FRI commitment chain: $approx.eq log_2 (8192) = 13$ layers, each with its
  own Merkle commitment
- Query proofs: ~20--32 queries, each contributing Merkle authentication
  paths through the trace + constraint + each FRI layer tree

Predicted serialised proof size: *~40--80 KB*. Point estimate *50 KB*.

== Verify time on Cortex-M33

Dominant work during STARK verify:

+ Hash the proof's Merkle roots → fixed small cost, $<$ 1 ms
+ Verify ~20 query paths into the trace commitment → 20 queries × 13
  tree depth × 2 children hashed per step ≈ 500--600 Blake3 compressions
+ Verify ~20 query paths into each of 13 FRI layer commitments →
  20 × 13 × small depth ≈ another 500--800 hashes
+ Field arithmetic over Goldilocks: a few hundred 64-bit multiplications
  + reductions during FRI folding consistency checks and constraint
  evaluation at out-of-domain (OOD) points. Cheap compared to hashes.
+ Final constraint evaluation at the OOD point — a handful of multiplies

Blake3 compression cost on Cortex-M33 at 150 MHz: ~5000--8000 cycles
per 64-byte compression in a pure-Rust build (no SIMD). Goldilocks
mul: estimated ~30--50 cycles on a 32-bit MCU (64-bit schoolbook via
32×32 = 64 native muls × 4 + reduction).

Rough budget:

#compare-table(
  ("Component", "Count", "Cost each", "Total"),
  (
    ([Blake3 compressions], [~1200], [~45 μs], [~54 ms]),
    ([Goldilocks field ops], [~500], [~0.3 μs], [~0.15 ms]),
    ([FRI layer consistency checks], [13], [~1 ms], [~13 ms]),
    ([Constraint OOD evaluation], [1], [~5 ms], [~5 ms]),
    ([Parser + scratch], [1], [~10 ms], [~10 ms]),
  ),
)

Point estimate: *~80--100 ms*. Wide interval because these
sub-estimates are themselves uncertain — real Blake3 on Cortex-M33
could be 2× slower than the 45 μs / compression figure if the ARM
backend doesn't vectorise 32-bit operations as tightly as the
benchmark I'm extrapolating from.

Final prediction: *150--400 ms on Cortex-M33.*

== Verify time on Hazard3 RV32

STARK verify is hash + field arithmetic, almost no G1/G2 work. Phase
2.6 showed M33 wins big on Fq12 tower ops (pairing) but that's
irrelevant here — STARKs don't touch Fq12. The relevant primitives:

- *Blake3*: Cortex-M33 has `UMAAL` which could help Blake3's 32-bit
  mixing, but Blake3 is mostly 32-bit XOR + ADD + ROT — not multiply-
  heavy — so `UMAAL` probably doesn't matter much. RV32 within 10 %.
- *Goldilocks mul*: 64-bit prime on 32-bit hardware. RV32's 31 GP
  registers *might* help the schoolbook 64-bit mul's register spill
  count, similar to the original BN254 G1-mul observation. But this
  was also the finding that didn't survive a `substrate-bn` dep bump,
  so *caveat heavily*.

Prediction: *RV32 verify time within ±30 % of M33*, with M33 slightly
favoured because Blake3 is M33's natural register-count case (Blake3 fits
its whole state in 16 32-bit registers).

Point estimate: *180--500 ms on Hazard3 RV32*. Same wide interval as M33
with a slight upward bias.

== Memory

Verifier state during a single `verify` call needs to hold:

- The proof buffer (owned by the caller, but must be live in RAM):
  40--80 KB
- Deserialised proof structure: possibly 2× the serialised size due
  to unpacked Merkle paths and field element representations
- Query-path working set: small, ~1--2 KB
- FRI layer scratch: ~5--10 KB
- Blake3 state: 64 bytes per active hasher

#compare-table(
  ("Quantity", "Predicted value"),
  (
    ([Proof buffer], [~50 KB]),
    ([Verify working heap], [~50--80 KB]),
    ([Peak stack during verify], [~18--30 KB]),
    ([Total RAM during verify], [~120--180 KB]),
  ),
)

*Tier implications.* Total RAM of 120--180 KB fits on the 256 KB SRAM
tier but *likely not on the 128 KB tier* that Groth16 / BN254 sits
on. This is the most consequential uncertainty — if STARK verify needs
more than 128 KB total RAM, the deployment story changes from
"ships on ST33 / nRF52832 / STM32F405" to "ships on nRF52840 /
STM32H7 / Ledger ST33K1M5". Both tiers are commercially relevant
but the 128 KB story is tighter for the hardware-wallet narrative.

== Variance

Predict *0.03--0.1 % iteration-to-iteration variance*. Same silicon,
same in-order pipeline, same single-tenant environment that produced
0.030 % on Semaphore and 0.07 % on baseline Groth16. STARK verify
doesn't introduce any obvious jitter source — the hashing is
data-oblivious in control flow, and the FRI query set is fixed per
proof.

= Falsification criteria

The following outcomes would invalidate the predictions above:

- *Full verify on M33 outside 80--600 ms*. Below 80 ms suggests Blake3
  on Cortex-M33 is substantially cheaper than the extrapolated 45 μs /
  compression, which would be a reproducibility finding in its own
  right. Above 600 ms suggests either the hash count is higher than
  estimated (a winterfell-internal detail I'm getting wrong), or
  Goldilocks arithmetic on the M33 is much worse than predicted.
- *Proof size outside 30--150 KB*. Below 30 KB would suggest
  winterfell uses more aggressive proof compression at default
  parameters than I'm assuming. Above 150 KB would suggest the
  default security parameters pull more queries than 96-bit STARK
  security typically needs.
- *Peak heap exceeds 200 KB*. Would disqualify the 256 KB SRAM tier
  and force either (a) tuning winterfell security parameters down or
  (b) implementing streaming verify that doesn't buffer the whole
  proof. Publishable either way.
- *RV32 / M33 verify ratio outside 0.7--1.3×*. Would mean hash +
  Goldilocks on the two ISAs diverge more than the cross-ISA pattern
  on BN254 / BLS12 predicted.

= Phase 3.1 deliverables (in build order)

+ *This document.*
+ `crates/zkmcu-verifier-stark`: `no_std` wrapper around `winterfell`
  that exposes a zkmcu-shaped API (`parse_proof`, `verify`). Direct
  sub-deps on `winter-verifier` + `winter-fri` + `winter-crypto` +
  `winter-math` to keep `winter-prover` out of the verify-only firmware
  build graph.
+ Host-side Fibonacci prover: a thin CLI using `winter-prover` that
  emits a deterministic Fibonacci proof for $N = 1024$ under fixed
  seeds. Output: `crates/zkmcu-vectors/data/stark-fib-1024/proof.bin`
  + `public.bin`. (No `vk.bin` because STARKs don't have a VK — the
  AIR definition is the verifier-side invariant.)
+ Host-side cross-check test: re-verify the bytes using `winter-verifier`
  before they hit disk, mirror of the BLS12-381 arkworks-zkcrypto
  cross-check pattern.
+ `bench-rp2350-m33-stark` + `bench-rp2350-rv32-stark` firmware crates.
  Minimum change from the existing `bench-rp2350-m33-bls12` template:
  swap crate dep, swap vector, swap the primitive-timing section (STARK
  doesn't have G1/G2/pairing to time separately — time parse + verify
  as a single block).
+ First measurement runs under `benchmarks/runs/`.
+ Comparison report `research/reports/<date>-stark-results.typ`
  quoting this document's predictions and stating measured values.
  This prediction document does not get edited after commit.

= Explicit non-claims

- *Not a claim that this is the best STARK verifier possible on MCU.*
  Winterfell's Fibonacci path is the reference "hello world", not an
  optimised embedded target. A custom minimal-STARK implementation
  could plausibly be 2--5× faster, but that's Phase 4 work.
- *Not a claim that this benchmark proves STARK verify fits the 128 KB
  SRAM tier.* The prediction explicitly expects total RAM of
  120--180 KB, landing on the 256 KB tier. If the measurement shows
  $<$ 128 KB, that's a publishable win; if it shows > 128 KB, the
  narrative shifts to "256 KB tier" which is still viable
  commercially (nRF52840, STM32H7).
- *Not a claim that Goldilocks is the right field for embedded.*
  BabyBear or Mersenne-31 would plausibly be faster on 32-bit MCU
  inner loops. Goldilocks is the winterfell default and therefore the
  path of least resistance for phase 3.1. Field comparison is a
  phase-3 follow-up.
- *Not a claim that Fibonacci is a realistic workload.* It's the
  STARK hello-world. Realistic workloads (Miden VM trace verify,
  RISC-V zkVM proof verify, aggregation) will be substantially more
  expensive. Phase 3.2 territory.

= Open questions to resolve during Phase 3.1

- *Umbrella vs direct sub-deps.* Does depending on
  `winter-verifier` + `winter-fri` + `winter-crypto` + `winter-math`
  directly eliminate `winter-prover` from the firmware build graph,
  or does the umbrella re-export them in a way that drags prover
  along? 10-minute follow-up spike. If the direct form works, firmware
  flash + RAM usage shrinks.
- *Proof streaming.* If the proof is 50 KB and the verify working set
  is another 80 KB, we're at 130 KB just for verify state. Worth
  checking whether winterfell's verifier can consume proof bytes
  incrementally rather than requiring the whole buffer live — could
  reduce peak RAM significantly. Probably not supported by the
  public API as-is; would need a fork or a custom wrapper.
- *Parallel hash option.* Winterfell has a `concurrent` feature for
  parallel Merkle verification via `rayon`. Disabled under
  `default-features = false`, as desired for no_std. But if there's
  an opportunity to use the M33 + RV32 dual-core RP2350 for parallel
  hash work at the firmware level, that's a phase-4 optimisation.

#v(1em)

_Prior-art survey_: `research/notebook/2026-04-23-stark-prior-art.md`.

_Dep-fit spike_: `research/notebook/2026-04-23-stark-dep-fit/`.
