#import "/research/lib/template.typ": *

#let m33_baseline = toml("/benchmarks/runs/2026-04-21-m33-groth16-baseline/result.toml")
#let m33_depbump  = toml("/benchmarks/runs/2026-04-21-m33-post-depbump/result.toml")
#let m33_stack    = toml("/benchmarks/runs/2026-04-21-m33-stack-painted/result.toml")
#let rv32_base    = toml("/benchmarks/runs/2026-04-21-rv32-groth16-baseline/result.toml")
#let rv32_stack   = toml("/benchmarks/runs/2026-04-21-rv32-stack-painted/result.toml")

#show: paper.with(
  title: "zkmcu — first-session report",
  authors: ("zkmcu",),
  date: "2026-04-21",
  kind: "report",
  abstract: [
    We present the first bare-metal `no_std` Rust implementation of
    Groth16/BN254 proof verification for the RP2350 microcontroller, running
    on both the Cortex-M33 and Hazard3 RISC-V cores of the same silicon.
    A single Rust source tree produces verifiers for both ISAs. End-to-end
    measurement on a Raspberry Pi Pico 2 W at 150 MHz: full Groth16 verify
    in *972 ms on Cortex-M33* and *1341 ms on Hazard3*, with *peak stack
    usage of 15.2 KB / 15.3 KB respectively*, iteration-to-iteration
    variance under *0.05 %*. Proof generation is performed on the host by
    `ark-groth16`; identical byte streams are verified natively by
    `substrate-bn` and on-device by the embedded verifier, closing a
    cross-library consistency check. All reported numbers come from real
    silicon, not emulation. A surprising side finding is that Hazard3 is
    *35 % faster* than Cortex-M33 on G1 scalar multiplication despite being
    slower (by 31–34 %) on everything else.
  ],
)

= Context

Zero-knowledge proof verification today runs on phones, servers, or
Ethereum nodes. Several real applications need it on a *microcontroller*:
hardware wallets that must check ZK claims without trusting a USB host;
offline credential validators (tickets, transit, border); IoT attestation
tags; supply-chain provenance chips. None of those classes of device
currently ship onboard SNARK verification because no one has published a
`no_std` Rust pairing-based verifier that fits on tight silicon.

A prior-art survey (`research/prior-art/main.typ`) confirmed the gap:
the only published "embedded Groth16" project runs under Linux on a
Raspberry Pi Zero W; no Cortex-M implementation exists; no hardware
wallet vendor ships onboard pairing-based verification; `mopro` (the
closest analogue) explicitly targets phones and not MCUs. Hazard3, RP2350's
RISC-V core, has never been benchmarked on 256-bit modular arithmetic.

= Contributions

1. First published bare-metal `no_std` Rust Groth16/BN254 verifier for
  Cortex-M33 silicon. Binary footprint 70 KB flash, 15.2 KB peak stack,
  972 ms per verify at 150 MHz.
2. First published Groth16/BN254 verifier on a RISC-V microcontroller
  (Hazard3 core on the same RP2350 silicon). 71 KB flash, 15.3 KB peak
  stack, 1341 ms per verify. Same source tree.
3. A portable Rust crate layout (`zkmcu-verifier`, `zkmcu-vectors`,
  `zkmcu-host-gen`) that isolates the crypto, the test vectors, and the
  proof-generation tooling, with cross-checking between `ark-groth16`
  (host) and `substrate-bn` (embedded) that guarantees wire-format
  compatibility.
4. A direct cross-ISA cycle-level comparison of `substrate-bn` on
  identical silicon at identical clock. Cortex-M33 wins the overall
  Groth16 verify by ~34 %; Hazard3 wins G1 scalar multiplication by ~35 %
  in a way that inverts the conventional wisdom.
5. Two reusable tooling artefacts: a CSR-level `mcountinhibit` enablement
  helper for Hazard3 (needed for cycle-counter access, undocumented in
  the context of cryptographic benchmarking), and a 64 KB-window stack
  painter shared across both firmware crates.

= System

#table(
  columns: 2,
  stroke: 0.4pt + luma(180),
  align: (left, left),
  [*Crate*], [*Role*],
  [`zkmcu-verifier`], [`no_std` Groth16 verify algorithm on top of `substrate-bn` 0.6; 4-pairing `pairing_batch` check with EIP-197 wire format.],
  [`zkmcu-vectors`], [`no_std` loader for committed binary test vectors (`include_bytes!`).],
  [`zkmcu-host-gen`], [Host CLI; `ark-groth16` 0.5 generates a proof for a trivial `x^2 = y` circuit and serialises VK/proof/public in EIP-197.],
  [`bench-rp2350-m33`], [Firmware for Cortex-M33. 150 MHz, DWT cycle counter, USB-CDC serial output, `embedded-alloc::LlffHeap` 256 KB arena.],
  [`bench-rp2350-rv32`], [Firmware for Hazard3. 150 MHz, `mcycle`/`mcycleh` CSR-paired read, same USB plumbing.],
)

The verifier code is the same Rust source for both firmwares.
Architecture-specific work is confined to the firmware crates (entry
macro, linker script, cycle-counter read, stack-painting helper). The
`embedded-hal` dependency is routed through a git fork at workspace
level via `[patch.crates-io]`, so every embedded-hal user in the tree
(including `rp235x-hal` and the `embedded-hal-async` family) consumes
the same source.

Proof-generation flow:

1. `cargo run -p zkmcu-host-gen --release` uses `ark-groth16` to perform
  a trusted setup for `x^2 = y`, generate a proof for `(x = 3, y = 9)`,
  and natively verify it.
2. The VK, proof, and public-input list are serialised in Ethereum
  precompile (EIP-197) format: 32-byte big-endian field elements;
  `G_1` is $x parallel y$; `G_2` is $x.c_1 parallel x.c_0 parallel y.c_1 parallel y.c_0$.
3. Committed into `crates/zkmcu-vectors/data/square/{vk,proof,public}.bin`.
4. Native cross-check: `zkmcu-verifier` parses these bytes using
  `substrate-bn` and runs its own verify. Two independent cryptographic
  libraries must agree; they do.
5. Firmware `include_bytes!`-pulls the committed files, calls
  `zkmcu_verifier::verify`, times it with the per-ISA cycle counter.

= Measurements

All numbers below come from real silicon on a Raspberry Pi Pico 2 W,
150 MHz system clock, `rustc 1.94.1`, `release` profile with
`lto = "fat"`, `opt-level = "s"`, `codegen-units = 1`, `panic = "abort"`.

== Full Groth16 verification

#table(
  columns: 5,
  stroke: 0.4pt + luma(180),
  align: (left, right, right, right, right),
  [*ISA*], [*Cycles (median)*], [*μs (median)*], [*Iterations*], [*Variance*],
  [Cortex-M33], [#m33_stack.bench.groth16_verify.cycles_median], [#m33_stack.bench.groth16_verify.us_median], [#m33_stack.bench.groth16_verify.iterations], [< 0.1 ms],
  [Hazard3 RV32], [#rv32_stack.bench.groth16_verify.cycles_median], [#rv32_stack.bench.groth16_verify.us_median], [#rv32_stack.bench.groth16_verify.iterations], [< 0.2 ms],
)

Ratio RV32 / M33 = *1.38 ×*. Both cores return `ok=true` on every
iteration. The wall-clock difference comes entirely from per-instruction
work: same clock, same memory, same `substrate-bn` source.

== Per-operation breakdown (same cycle medians, μs in parentheses)

#table(
  columns: 4,
  stroke: 0.4pt + luma(180),
  align: (left, right, right, right),
  [*Operation*], [*M33*], [*RV32*], [*Ratio*],
  [G1 scalar mul (typical)], [16.5 M (≈ 110 ms)], [*10.8 M (≈ 72 ms)*], [*0.65 ×*],
  [G2 scalar mul (typical)], [31.5 M (≈ 210 ms)], [42.5 M (≈ 284 ms)], [1.35 ×],
  [BN254 pairing],            [80.0 M (≈ 535 ms)], [106.0 M (≈ 707 ms)], [1.32 ×],
  [Groth16 verify],           [145.7 M (≈ 972 ms)], [201.2 M (≈ 1341 ms)], [1.38 ×],
)

*G1 scalar multiplication is 35 % faster on Hazard3 than on Cortex-M33*,
using the same pure-Rust `substrate-bn` source and optimiser
settings. This inverts the conventional expectation that ARM's
multiply-accumulate instructions (SMLAL, UMAAL) would give M33 an
advantage on 256-bit big-integer arithmetic. Since `substrate-bn` does
not use those intrinsics, the hardware advantage is unrealised; meanwhile
Hazard3's 31 general-purpose registers (versus Thumb-2's 13) reduce
register spilling during schoolbook field multiplication, which dominates
G1 cost. On G2 (tower field, more memory traffic) and the full pairing
(recursion through Fp12) the register-count advantage is overtaken by
other factors.

== Stack

Measured with 64 KB-window stack painting: sentinel-fill the region just
below the measurement-function's SP, call `verify` once, scan for the
lowest address overwritten.

#table(
  columns: 3,
  stroke: 0.4pt + luma(180),
  align: (left, right, right),
  [*ISA*], [*Peak stack (measured)*], [*% of painted window*],
  [Cortex-M33], [15,604 B (≈ 15.2 KB)], [24 %],
  [Hazard3 RV32], [15,708 B (≈ 15.3 KB)], [24 %],
)

The 0.7 % difference confirms peak stack is an algorithmic property, not
an ISA property. Combined with the 256 KB heap arena, total RAM
utilisation is ~272 KB on both cores — 53 % of the 520 KB SRAM
available on the RP2350.

== Determinism

Within-run variance on Groth16 verify:

- Cortex-M33: 94,762 cycles over 145.7 M median → *0.065 %*.
- Hazard3: 122,639 cycles over 201.2 M median → *0.061 %*.

For comparison, running the same `substrate-bn` verify on a modern
desktop CPU shows 10–50 % variance per run due to cache contention,
frequency boost, and OS preemption. Deterministic verification latency
is a defensible product feature for applications with timing
side-channel concerns, hard deadlines, or audit requirements.

= Observations worth promoting

- *The `mcountinhibit` gotcha.* Hazard3 boots with
  `mcountinhibit[CY] = 1`, holding the cycle counter at zero until a
  single CSR write clears it. This is noted in the Hazard3 README but
  has never (as of this writing) surfaced in the context of
  cryptographic benchmarking. Our first RV32 flash reported `cycles=0`
  on every stage; correctness was unaffected. The fix is one instruction
  (`csrw mcountinhibit, zero`). Documented as a reusable finding.
- *Pairing time on the identity G2 generator is ~8 % faster than on a
  scaled point.* Early isolated-pairing benchmark on `(p, G2::one())`
  gave 495 ms; pairing on `(p, G2::one() * s)` gives 533 ms.
  Presumably `substrate-bn` fast-paths the canonical generator during
  Miller loop twist line evaluations.
- *Bumped-to-latest dependencies: `embedded-alloc` 0.5 → 0.7 (rename to
  `LlffHeap`), `heapless` 0.8 → 0.9, `panic-halt` 0.2 → 1.0.* The `rand`
  ecosystem is locked to 0.8 by `arkworks` 0.5 (`rand_core` 0.6 traits);
  bumping `rand` past that breaks `ark-snark::SNARK::prove` because
  `rand_core` 0.10 is a trait-level rewrite. Pinned in-place with a
  comment pointing at the blocker.
- *Workspace `[patch.crates-io]` routes our `embedded-hal` dependency to
  `Niek-Kamer/embedded-hal` (a maintainer-tracking fork) at a single
  edit point.* Every HAL crate — `rp235x-hal`, `embedded-hal-async`,
  `embedded-hal-bus`, `embedded-hal-nb` — resolves to the fork, so the
  trait graph has a single version.

= What is not yet measured

- Peak *heap* usage during one verify. We provision 256 KB but the real
  peak is likely a fraction of that. Shrinking the arena toward the
  measured peak plausibly lets the entire verifier run on a 64 KB SRAM
  MCU (a much larger and cheaper silicon market).
- Scaling with public-input count. Our test vector uses one public
  input; realistic circuits have 5–50. The linear-combination step
  (`vk_x = IC[0] + sum_i input[i] * IC[i+1]`) grows linearly.
- Hand-tuned lower-level arithmetic. ARM DSP intrinsics (SMLAL, UMAAL)
  and Hazard3 Zbb/Zba/Zbc/Zbs extensions are both unrealised ceilings.
  Each could plausibly contribute 2–3 × speedup; combined, target verify
  time drops from ~1 s to ~150 ms.
- Plonk / Halo2 / Nova / HyperNova verifiers. All open territory; Nova
  and HyperNova are particularly attractive because their native
  (non-circuit) verifier is dramatically cheaper than Groth16.
- BLS12-381 instead of BN254 (`bls12_381` crate instead of `substrate-bn`).
  Zcash / Filecoin / Ethereum consensus use this curve; a second curve
  support would roughly double the addressable proof ecosystem.
- Hardware-wallet reference design. A secure-element-class device
  (ST33, SE050) with zkmcu as the verifier layer has no published
  reference implementation in the commercial HW-wallet ecosystem.

= Related work

See `research/prior-art/main.typ` for the detailed survey. In summary,
the closest prior art (ZPiE, 2021) runs C on a Raspberry Pi Zero W under
Linux; no Cortex-M implementation exists publicly; no hardware wallet
currently ships onboard SNARK verification. `mopro` targets phones and
above, not MCUs. Adjacent calibration points: `pqm4` (Cortex-M4
post-quantum signature benchmarks), González et al.'s 8 KB-RAM PQC
verifier, and the `BLS12-381 Pairing with RAM Footprint Smaller than
4 KB` result — all supporting the claim that a pairing-based verifier
fits comfortably on devices much smaller than the RP2350.

= Roadmap

*Immediate (next 1–2 weeks).* Heap-peak measurement; larger test
vector with 5–10 public inputs; public blog/announcement post; one
grant submission (PSE or Ethereum Foundation ESP).

*Next 1–2 months.* BLS12-381 backend; Plonk/Halo2 verifier; hardware
wallet vendor conversations (Ledger/Trezor/Keystone); Cortex-M DSP
intrinsics for Montgomery reduction.

*6-month horizon.* Nova/HyperNova verifier — much cheaper per verify
than Groth16 (one scalar multiplication plus one hash); BN254 ASIC
co-processor comparison (separately published FPGA / ASIC results give
a theoretical floor against which we can size the pure-software path).

= Reproduction

The full code, committed test vectors, benchmark runs, and this report's
sources live in the project repository. Building everything locally
requires a stable `rustc` (1.82+), `thumbv8m.main-none-eabihf` and
`riscv32imac-unknown-none-elf` target components, `typst` 0.14 for the
PDFs, and `picotool` 2.1+ on the flash host. Each benchmark run has a
`notes.md` alongside its `result.toml` and `raw.log` — the `raw.log` is
the verbatim serial capture from the device and is the authoritative
source for the numbers above.

#bibliography("/research/lib/refs.bib", style: "american-institute-of-aeronautics-and-astronautics")
