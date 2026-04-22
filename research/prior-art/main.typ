#import "/research/lib/template.typ": *

#show: paper.with(
  title: "Prior art — embedded SNARK verification",
  authors: ("zkmcu",),
  date: "2026-04-21",
  kind: "survey",
  abstract: [
    A survey of the existing public work on verifying zero-knowledge
    proofs on microcontrollers. Finds a clear gap: no published bare-metal
    `no_std` Rust Groth16 verifier for any Cortex-M target exists; the closest
    prior work (ZPiE, 2021) is C on a Linux-class Raspberry Pi Zero, and no
    shipping hardware wallet currently verifies SNARKs onboard.
  ],
)

= Overview

This document is a living survey. It is updated whenever a new candidate
"someone has already done this" claim surfaces. Entries are grouped by
what they actually demonstrate, not by keyword overlap.

== Claims in scope

We care about software (or software-defined hardware) that verifies a
succinct zero-knowledge proof — Groth16, Plonk/Halo2/KZG variants,
STARKs wrapped in Groth16, Nova/HyperNova — on a *constrained device*:
something smaller than a Linux-class SoC. The Raspberry Pi Zero is the
boundary where we consider the target "embedded enough to matter."

= Direct prior art

== ZPiE — Zero-knowledge Proofs in Embedded systems

@zpie2021. C, Groth16 over BN128, Raspberry Pi Zero W (ARM11 @ 700 MHz,
Linux). The only published "embedded Groth16" project in the record.
Not `no_std`, not Rust, not Cortex-M, not bare-metal. Runs under glibc.
Useful as a reference implementation; does not close the gap we are
targeting.

== Hardware wallets

Ledger (BOLOS on ST33/STM32), Trezor, Keystone, OneKey, Tangem — as of
survey date, *none* ship onboard SNARK verification. Their precompiled
crypto is ECDSA/EdDSA/ECDH plus symmetric primitives. Pairing-friendly
curves are absent from commercial Secure Elements. Any wallet that
"supports ZK" today delegates verification off-device.

== Other embedded-ZK research

- `ZKSA` (Computers & Security, 2024), `ZEKRA` (ACM AsiaCCS 2023): IoT
  attestation schemes using SNARKs; prototypes on Raspberry Pi 3B
  (Linux). Verifier-on-MCU is not demonstrated.
- "Pairings and ECC for Embedded Systems" @unterluggauer2014: software
  pairings on Cortex-M0+. Good performance floor, no high-level
  SNARK integration.
- "Low-Power BLS12-381 Pairing Cryptoprocessor for IoT"
  (IEEE JSSC 2021): *custom ASIC*, not software. Useful as a
  theoretical lower bound on what dedicated silicon can achieve.
- "BLS12-381 Pairing with RAM Footprint Smaller than 4 KB"
  (IEEE 2022): software pairing on Cortex-M0+ (!). Demonstrates that
  the *primitive* fits on far smaller silicon than RP2350.

= Adjacent reference points

- *Mopro* @mopro: PSE mobile prover, UniFFI bindings for Circom and
  Halo2. Targets phones; does not go below. Explicitly confirms the
  MCU niche is unaddressed.
- *pqm4* (Kannwischer et al.): the de facto benchmark framework for
  post-quantum signature verification on Cortex-M4. Dilithium2 verify
  in ~3 KB stack, ~13 ms at 24 MHz. Shows compact verify on small
  MCUs is tractable; a useful calibration point.
- @gonzalez2021: streams Dilithium/Falcon/SPHINCS+ verification into
  a Cortex-M3 with 8 KB of RAM. Closest analogue in engineering
  spirit.

= Gaps zkmcu addresses

1. *No published bare-metal `no_std` Rust Groth16 verifier on
   any Cortex-M.* zkmcu's first public artifact closes this.
2. *No RP2350 pairing-crypto work.* wolfSSL ships symmetric and
   ECDSA primitives on the chip; no pairing library targets it.
3. *No reported stack / heap footprint of arkworks' `ark-groth16`
   verify on a Cortex-M.* All existing evaluations run on Linux.
4. *No Hazard3 / RV32 benchmark for pairing-grade 256-bit modular
   arithmetic.* RISC Zero, SP1, Jolt treat RISC-V as a *proving*
   ISA, not real-silicon verification hardware.
5. *No open reference design for onboard SNARK verification on a
   Secure Element-class device.*

= Status of the gap (as of 2026-04-21)

Items 1–4 above are *closed* by the zkmcu artefacts shipped alongside
this survey:

- *Cortex-M33 verifier.* `bench-rp2350-m33`, 70 KB flash, 15.2 KB peak
  stack, 972 ms per Groth16 verify at 150 MHz. See
  `research/reports/2026-04-21-zkmcu-first-session.typ` for the complete
  measurement.
- *Hazard3 RV32 verifier.* `bench-rp2350-rv32`, 72 KB flash, 15.3 KB peak
  stack, 1341 ms per Groth16 verify at the same clock. Same Rust source
  tree; only the entry macro, linker script, and cycle-counter access
  differ between builds.
- *Footprint measured directly.* Stack painting at boot: sentinel-fill a
  64 KB window below current SP, run one verify, scan for the lowest
  address overwritten. Reported peak is the fully-measured figure with
  the paint margin added back.
- *Cross-ISA arithmetic comparison.* Same `substrate-bn` source on
  identical silicon at identical clock; full numerical table in the
  session report.

Item 5 (hardware-wallet reference design) is unclaimed and remains open.
zkmcu's artefact is a portable verifier library plus two firmware
binaries, not a secure-element-class board; integrating zkmcu into a
wallet vendor's production firmware is the natural next step for that
specific gap.

Items generated by zkmcu that *other* projects may find useful:

- *Hazard3 `mcountinhibit` enablement.* Documented for the first time in
  a cryptographic-benchmarking context. One CSR write (`csrw mcountinhibit, zero`)
  is required at boot to make any cycle-level measurement on Hazard3
  meaningful.
- *EIP-197 bridge between `arkworks` and `substrate-bn`.* A clean
  encoding / parser pair tested against both stacks; downstream no-std
  users of BN254 can reuse it.

= Calibration points

Dilithium2 verification fits in ~3 KB stack and completes in ~13 ms on
a 24 MHz Cortex-M4. BLS12-381 software pairing runs in under 4 KB of
RAM on a Cortex-M0+. RP2350 at 150 MHz with 520 KB SRAM is
over-provisioned for Groth16 verification by every measurable axis.
The challenge is not whether it fits — it is publishing the exact
numbers, `no_std`-clean, in Rust.

// TODO(survey-update): add any published competitor, hardware wallet
// announcement, or peer-reviewed result that changes the gap claim.
// Keep the bibliographic entries in `research/lib/refs.bib`.

#bibliography("/research/lib/refs.bib", style: "american-institute-of-aeronautics-and-astronautics")
