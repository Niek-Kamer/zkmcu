#import "/research/lib/template.typ": *

#let m33 = toml("/benchmarks/runs/2026-04-21-m33-stack-painted/result.toml")
#let rv32 = toml("/benchmarks/runs/2026-04-21-rv32-stack-painted/result.toml")

#show: paper.with(
  title: "zkmcu — SNARK verification on microcontrollers",
  authors: ("zkmcu",),
  date: "2026-04-21",
  kind: "whitepaper",
  abstract: [
    zkmcu is an open-source `no_std` Rust implementation of Groth16 / BN254
    proof verification for ARM Cortex-M and RISC-V microcontrollers. It
    targets use cases — hardware-wallet-local ZK, IoT attestation, offline
    credential checks, supply-chain provenance — where the relying party is
    itself a constrained device. The first public measurement (RP2350 at
    150 MHz) verifies a Groth16 proof in *972 ms on Cortex-M33* and *1341 ms
    on Hazard3 RV32*, with *peak stack usage under 16 KB* on both cores and
    iteration-to-iteration variance under *0.07 %*. Proof generation is
    performed on a host with `ark-groth16`; the embedded verifier uses
    `substrate-bn`; identical bytes flow between the two, which pins down
    wire-format compatibility. The project fills a documented gap in the
    prior-art record: no bare-metal `no_std` Rust pairing-based verifier
    for Cortex-M or Hazard3 has been published.
  ],
)

= Problem

Zero-knowledge proofs are verified today on phones, servers, or
blockchain nodes. An underserved class of applications needs on-device
verification on *constrained hardware* — devices with hundreds of KB of
RAM and single-digit-watt power envelopes, not gigabytes and gigahertz.

- *Hardware wallets* that check a ZK claim (anonymous credentials,
  private balances, selective disclosures) *locally*, without trusting
  the USB host they are connected to.
- *Offline ticket and credential validators.* Festival gates, transit
  turnstiles, border checkpoints. Must verify privacy-preserving
  credentials with no server.
- *IoT attestation.* A sensor proves to a neighbouring device that it is
  genuine hardware and its reading has not been tampered with — without
  a cloud service in the middle.
- *Supply-chain provenance tags.* A \$2 chip on a crate verifies that a
  provenance certificate is cryptographically valid before forwarding.

These classes are unserved by existing tooling. Phones run the mobile
prover ecosystem (`mopro`, the Privacy & Scaling Explorations stack) but
those tools explicitly target mobile and up. Hardware wallets implement
ECDSA/EdDSA and symmetric primitives in firmware; they do not verify
SNARKs. The only published "embedded" Groth16 (ZPiE, 2021) runs C under
Linux on a Raspberry Pi Zero W, which is not a microcontroller in the
sense this paper uses the term.

= Design

== Wire format

zkmcu uses the *EIP-197* wire format for BN254 VKs, proofs, and public
inputs: 32-byte big-endian field elements; `G1` as `x ‖ y`; `G2` as
`x.c1 ‖ x.c0 ‖ y.c1 ‖ y.c0`. The format is ubiquitous (every Ethereum
precompile agrees on it) and requires no additional encoding/decoding
layer for common production proofs.

Container formats:

- *Verifying key:* `alpha` (G1) ‖ `beta` (G2) ‖ `gamma` (G2) ‖ `delta` (G2)
  ‖ num_ic (u32 LE) ‖ ic[num_ic] (G1). Typical size: 580 B for a
  one-public-input circuit.
- *Proof:* A (G1) ‖ B (G2) ‖ C (G1). Always 256 B.
- *Public inputs:* count (u32 LE) ‖ input[count] (Fr). 4 + 32·count B.

== Verification algorithm

zkmcu implements the canonical Groth16 check over the product of four
pairings. Given a verifying key `(α, β, γ, δ, IC)`, a proof `(A, B, C)`,
and public inputs `x[0..n]`:

1. Compute the public-input linear combination `vk_x = IC[0] + Σ x[i] * IC[i+1]`, a point in G1.
2. Check that `e(-A, B) * e(α, β) * e(vk_x, γ) * e(C, δ)` equals the
  identity element in the target group GT.

The four pairings are computed together using `substrate-bn`'s
`pairing_batch`, which shares the final exponentiation across the Miller
loops. Empirically this costs ~2 × a single pairing, not 4 ×. The
implementation is 70 lines of Rust including parsing; the parsing is
defensive (bounds-checked `.get()`, no raw indexing) so a short or
malformed proof returns `Err` rather than panicking.

== Crate layout

#table(
  columns: 2,
  stroke: 0.4pt + luma(180),
  align: (left, left),
  [*Crate*], [*Role*],
  [`zkmcu-verifier`], [Pure Rust, `no_std`, uses `substrate-bn` for BN254 arithmetic. Exposes `parse_vk`, `parse_proof`, `parse_public`, and `verify`.],
  [`zkmcu-vectors`], [`no_std`, ships test vectors as committed `.bin` files via `include_bytes!`. The firmware pulls them at compile time.],
  [`zkmcu-host-gen`], [Host-only CLI. Uses `ark-groth16` + `ark-bn254` to run a trusted setup for a tiny circuit (`x^2 = y`), generate a proof, natively verify it, and serialise the artefacts into EIP-197 format.],
  [`bench-rp2350-m33`], [Firmware for Cortex-M33, 150 MHz, USB-CDC serial output, DWT cycle counter, 256 KB static heap arena.],
  [`bench-rp2350-rv32`], [Firmware for Hazard3 RV32, same board, `mcycle`/`mcycleh` CSR-paired read, same USB plumbing.],
)

== Cross-library consistency

The `zkmcu-verifier/tests/verify_square.rs` integration test loads the
committed binary files, parses them with `substrate-bn` types, and
verifies. An earlier step (the `zkmcu-host-gen` CLI) natively verified
the same proof via `arkworks` before serialising. Two independent
cryptographic libraries must agree on encoding and arithmetic; they do.
Any future change to either stack must keep this test green.

= Measurements

Hardware: Raspberry Pi Pico 2 W (RP2350, 520 KB SRAM, 4 MB QSPI flash),
150 MHz system clock.
Toolchain: `rustc` 1.94.1, `release` profile (`lto = "fat"`,
`opt-level = "s"`, `codegen-units = 1`, `panic = "abort"`).
Every number below was read off the device's USB-CDC serial output on
the chip itself; no emulation.

== Groth16 verify

#table(
  columns: 4,
  stroke: 0.4pt + luma(180),
  align: (left, right, right, right),
  [*Core*], [*Cycles (median)*], [*Wall time*], [*Variance*],
  [Cortex-M33], [#m33.bench.groth16_verify.cycles_median], [~#(m33.bench.groth16_verify.us_median / 1000) ms], [0.065 %],
  [Hazard3 RV32], [#rv32.bench.groth16_verify.cycles_median], [~#(rv32.bench.groth16_verify.us_median / 1000) ms], [0.061 %],
)

== Memory footprint (directly measured)

#table(
  columns: 3,
  stroke: 0.4pt + luma(180),
  align: (left, right, right),
  [*Region*], [*Cortex-M33*], [*Hazard3 RV32*],
  [`.text`], [#(m33.footprint.text_bytes / 1024) KB], [#(rv32.footprint.text_bytes / 1024) KB],
  [Heap (configured)], [256 KB], [256 KB],
  [Peak stack (measured)], [#m33.footprint.stack_peak_bytes B], [#rv32.footprint.stack_peak_bytes B],
  [Total RAM used], [~272 KB], [~278 KB],
)

Available SRAM on the RP2350 is 520 KB. zkmcu uses ~53 % of it with a
heap arena larger than necessary — shrinking the arena toward the
unmeasured heap peak plausibly brings total RAM below 64 KB, enabling
deployment on much cheaper silicon.

== Cross-ISA cycle comparison

#table(
  columns: 4,
  stroke: 0.4pt + luma(180),
  align: (left, right, right, right),
  [*Operation*], [*M33 cycles*], [*RV32 cycles*], [*RV32/M33*],
  [G1 scalar mul (typical)],    [16.5 M], [*10.8 M*], [*0.65 ×*],
  [G2 scalar mul (typical)],    [31.5 M], [42.5 M], [1.35 ×],
  [BN254 pairing],              [80.0 M], [106.0 M], [1.32 ×],
  [Full Groth16 verify],        [145.7 M], [201.2 M], [1.38 ×],
)

Surprise: Hazard3 beats Cortex-M33 on G1 scalar multiplication by 35 %.
Conventional wisdom is ARM's SMLAL / UMAAL multiply-accumulate
instructions dominate 256-bit big-integer work. `substrate-bn` is pure
Rust without intrinsic use, so those ARM advantages are not realised;
meanwhile Hazard3 has 31 general-purpose registers to Thumb-2's 13,
reducing register spills during schoolbook Fp multiplication. For the
higher-level operations (tower fields, pairing recursion) other factors
reverse the result. The gap leaves a clear optimisation ceiling for both
cores.

= Discussion

== What the numbers unlock

*At ~1 second per verify with 16 KB of stack*, zkmcu is fast enough for
every application in the problem statement. Hardware wallet ZK
verification must complete before the user notices the delay (< 2 s is
typical); offline credential validators are not latency-sensitive at
all; IoT attestation and supply-chain tags amortise verification over
seconds or minutes of idle time.

*At 0.06 % variance*, zkmcu has a defensible story for applications
where verification timing must be predictable: timing side-channel
mitigation, hard-deadline ticket validators, regulatory audit trails.
Variance this low is structural (in-order pipeline, fixed clock, single
tenancy) and cannot be replicated on a desktop CPU.

== Optimisation ceiling

Three independent optimisation axes remain open:

1. *ARM DSP intrinsics (Cortex-M33).* SMLAL, UMAAL, and the SIMD
  multiply variants are not used by `substrate-bn` today. A hand-tuned
  Montgomery reduction for BN254 using these intrinsics has been shown
  in adjacent work to give 2–3 × speedups over pure-Rust on equivalent
  silicon. Expected effect: verify drops from ~1 s to ~400 ms.
2. *RISC-V bit-manipulation (Hazard3).* `Zbb` (`clz`, `cpop`, `rev*`)
  and `Zba` (shifted add) help Montgomery reduction and point doubling.
  Currently unexploited by both `substrate-bn` and Rust codegen defaults
  for `riscv32imac`.
3. *Algorithmic.* Nova / HyperNova have native verifiers that are one
  scalar multiplication plus a hash, not four pairings — orders of
  magnitude cheaper. Supporting Nova would move zkmcu from "usable
  latency" to "sub-millisecond" for an equivalent proof.

Combined, a tuned zkmcu could plausibly verify a Groth16 proof in
~150 ms and a Nova proof in well under 10 ms on the same hardware.

= Related work

See `research/prior-art/main.typ` for the detailed survey. Summary:

- *ZPiE* (Salleras & Daza, 2021): the closest embedded Groth16
  implementation; C, Linux, Raspberry Pi Zero W. Not `no_std`, not Rust,
  not Cortex-M.
- *No hardware wallet (Ledger, Trezor, Keystone, OneKey, Tangem)*
  currently ships onboard SNARK verification.
- *`mopro`* (Privacy & Scaling Explorations): mobile prover stack; does
  not target MCUs.
- *Post-quantum signature verification on Cortex-M* (`pqm4`, González
  et al. 2021) demonstrates that compact verification is feasible on
  chips far smaller than the RP2350 — useful calibration points but
  different primitives.

= Future work

- Heap-peak measurement and arena shrinkage toward 64 KB SRAM targets.
- BLS12-381 backend (`bls12_381` crate): doubles the addressable proof
  ecosystem (Zcash, Filecoin, Ethereum consensus).
- Plonk / Halo2 verifier: no `no_std` MCU implementation exists.
- Nova / HyperNova: dramatically cheaper verifier; native implementation.
- DSP-intrinsic Montgomery reduction on Cortex-M33; Zbb-enabled build on
  Hazard3. Both are independently interesting microarchitecture papers.
- Hardware wallet reference design: a secure-element-class board with
  zkmcu integrated and one concrete demo flow (e.g., Ethereum privacy
  pool membership proof verified on-device).
- Open-source integration with real products: hardware wallets, IoT
  attestation frameworks, supply-chain provenance systems, ticket
  validators.

= Reproduction

All artefacts (source, committed test vectors, benchmark runs, PDF
sources) are in the project repository. Build and measure locally with
`just check-full` (formats, clippy, tests, firmware) and
`just docs` (PDFs). Each benchmark run in `benchmarks/runs/` is
self-contained: a `raw.log` serial capture, a structured `result.toml`,
and narrative `notes.md`.

#bibliography("/research/lib/refs.bib", style: "american-institute-of-aeronautics-and-astronautics")
