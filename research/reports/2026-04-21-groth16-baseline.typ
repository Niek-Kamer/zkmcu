#import "/research/lib/template.typ": *

#let run = toml("/benchmarks/runs/2026-04-21-m33-groth16-baseline/result.toml")

#show: paper.with(
  title: "Groth16/BN254 verify on RP2350 Cortex-M33 — baseline",
  authors: ("zkmcu",),
  date: "2026-04-21",
  kind: "report",
  abstract: [
    First published bare-metal `no_std` Rust Groth16/BN254 verification on
    an ARM Cortex-M. Measured on a Raspberry Pi Pico 2 W (RP2350,
    Cortex-M33 @ 150 MHz) using unmodified `substrate-bn` 0.6.0 with
    no platform-specific optimization: full Groth16 verify in
    *~988 ms*, stable to sub-millisecond across iterations.
  ],
)

= Setup

- Hardware: #run.hardware.board · #run.hardware.cpu @ #(run.hardware.clock_hz / 1000000) MHz · #(run.hardware.sram_bytes / 1024) KB SRAM · #(run.hardware.flash_bytes / 1048576) MB flash.
- Toolchain: #run.meta.toolchain.
- Profile: `#run.meta.profile`.
- Circuit: `#run.bench.groth16_verify.circuit` (#run.bench.groth16_verify.public_inputs public input, ic size #run.bench.groth16_verify.ic_size).
- Proof was generated on the host with `ark-groth16` #run.libraries."ark-groth16", serialized to EIP-197 @eip197 binary format, and embedded in the firmware.

= Results

== Groth16 verify

#run.bench.groth16_verify.cycles_median cycles · #run.bench.groth16_verify.us_median μs · result *#run.bench.groth16_verify.result* across #run.bench.groth16_verify.iterations iterations.
Cross-check: an arkworks-native verification of the same proof returns `true`; flipping a single bit of the public input causes our verifier to return `false`.

== Supporting measurements

#table(
  columns: 3,
  align: (left, right, right),
  stroke: 0.4pt + luma(200),
  [*Operation*], [*Cycles*], [*Wall time*],
  [BN254 pairing], [#run.bench.pairing.cycles_median], [#(run.bench.pairing.us_median / 1000) ms],
  [G1 scalar mul (typical)], [#run.bench.g1_mul.cycles_typical], [#(run.bench.g1_mul.us_typical / 1000) ms],
  [G2 scalar mul (typical)], [#run.bench.g2_mul.cycles_typical], [#(run.bench.g2_mul.us_typical / 1000) ms],
)

== Footprint

- `text`: #(run.footprint.text_bytes / 1024) KB
- `bss`:  #(run.footprint.bss_bytes / 1024) KB (of which #(run.footprint.heap_bytes / 1024) KB is the static heap)

= Discussion

The 988 ms figure is an *unoptimized* baseline. `substrate-bn` uses
pure-Rust 256-bit modular arithmetic with no DSP or Montgomery
intrinsics. Cortex-M33 exposes `SMLAL`/`UMAAL` for 32×32→64
multiply-accumulate that could plausibly reduce pairing time by
2–3×; that is the natural follow-up.

The multi-pairing is roughly 2× a single pairing rather than 4×,
consistent with `pairing_batch` sharing the final exponentiation
across the four Miller loops.

= Reproduction

See `benchmarks/runs/2026-04-21-m33-groth16-baseline/notes.md`.

#bibliography("/research/lib/refs.bib", style: "american-institute-of-aeronautics-and-astronautics")
