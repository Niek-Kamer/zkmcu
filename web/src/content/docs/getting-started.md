---
title: Getting started
description: Add zkmcu to a Rust project and verify your first proof.
---

zkmcu is a family of `no_std` Rust libraries. Two verifier crates, one per supported curve. Runs on host targets (for testing) and on embedded targets (its main job) from the same source tree.

## Pick a curve

| Curve | Crate | Wire format | Ecosystems |
|---|---|---|---|
| BN254 | `zkmcu-verifier` | EIP-197 | Ethereum L1 + all EVM L2s, Semaphore, Tornado Cash, MACI, Anon Aadhaar, Aztec* |
| BLS12-381 | `zkmcu-verifier-bls12` | EIP-2537 | Ethereum sync-committee, Zcash, Filecoin, Aleo, Celo* |

\* Protocols marked in italics may use a related-but-different curve or wire format; check the specific deployment.

Not sure? Groth16 on **Ethereum mainnet** and Ethereum-compatible chains is BN254. Anything involving **Ethereum's beacon-chain sync-committee**, **Zcash-style shielded transactions**, or **Filecoin PoRep / PoSt** is BLS12-381.

## Add to your `Cargo.toml`

```toml
[dependencies]
zkmcu-verifier       = "0.1"     # BN254
# or
zkmcu-verifier-bls12 = "0.1"     # BLS12-381
```

On an embedded target you also need a global allocator. `embedded-alloc` works well:

```toml
embedded-alloc = { version = "0.7", features = ["llff"] }
```

## Verify a proof

Simplest case — you have three byte buffers (verifying key, proof, public inputs) in the curve's wire format:

```rust
let ok = zkmcu_verifier::verify_bytes(&vk_bytes, &proof_bytes, &public_bytes)?;
// or the BLS12 equivalent:
let ok = zkmcu_verifier_bls12::verify_bytes(&vk_bytes, &proof_bytes, &public_bytes)?;

assert!(ok);
```

That's the whole API for one-shot verification. See [wire formats](/wire-format/) for what those buffers must look like on each curve.

## Verify many proofs against one VK

Parse the VK once, reuse it:

```rust
let vk = zkmcu_verifier::parse_vk(&vk_bytes)?;
for (proof_bytes, public_bytes) in batch {
    let proof  = zkmcu_verifier::parse_proof(&proof_bytes)?;
    let public = zkmcu_verifier::parse_public(&public_bytes)?;
    if zkmcu_verifier::verify(&vk, &proof, &public)? {
        // accept
    }
}
```

Identical shape for `zkmcu_verifier_bls12`.

## Generating test proofs

For development you need some proofs to verify against. The `zkmcu-host-gen` binary produces wire-format test vectors using `arkworks`:

```bash
cargo run -p zkmcu-host-gen --release               # both curves, synthetic vectors
cargo run -p zkmcu-host-gen --release -- bn254      # BN254 only
cargo run -p zkmcu-host-gen --release -- bls12-381  # BLS12-381 only
```

This writes `crates/zkmcu-vectors/data/<name>/{vk,proof,public}.bin`. Three vectors ship by default:

- `square` — 1 public input (BN254 + BLS12-381, two copies of the same circuit)
- `squares-5` — 5 public inputs, small-scalar (BN254 + BLS12-381)
- `semaphore-depth-10` — **real** Semaphore Groth16 proof, 4 public inputs, BN254 only. See [Semaphore page](/semaphore/) for the generator pipeline

For production use, generate proofs with whatever prover your application uses. Any Ethereum-compatible BN254 Groth16 prover emits EIP-197 bytes; any BLS12-381 Groth16 prover emits EIP-2537 bytes (or bytes convertible to EIP-2537 via a small shim if it's a non-Ethereum deployment like Zcash or Filecoin).

## On an embedded target

Four reference firmware crates ship in the repo. Pick the one that matches your curve + ISA:

- [`crates/bench-rp2350-m33/`](https://github.com/Niek-Kamer/zkmcu/tree/main/crates/bench-rp2350-m33) — ARM Cortex-M33, BN254
- [`crates/bench-rp2350-m33-bls12/`](https://github.com/Niek-Kamer/zkmcu/tree/main/crates/bench-rp2350-m33-bls12) — ARM Cortex-M33, BLS12-381
- [`crates/bench-rp2350-rv32/`](https://github.com/Niek-Kamer/zkmcu/tree/main/crates/bench-rp2350-rv32) — RISC-V Hazard3, BN254
- [`crates/bench-rp2350-rv32-bls12/`](https://github.com/Niek-Kamer/zkmcu/tree/main/crates/bench-rp2350-rv32-bls12) — RISC-V Hazard3, BLS12-381

Each brings up clocks, initialises a heap, parses baked-in test vectors via `include_bytes!`, runs `verify`, and prints results over USB-CDC serial. Same source for the crypto, different linker scripts and cycle-counter reads per ISA.

## What the target needs

- `no_std` Rust toolchain (stable, `rustc` 1.82 or newer)
- A global allocator
- About **97 KB of SRAM** during verify for either curve on Cortex-M33 (96 KB heap arena + 16-19 KB peak stack + small statics). Any 128 KB SRAM-class MCU works — nRF52832, STM32F405, Ledger ST33, Infineon SLE78.
- About **75 KB of flash** for the verifier + `substrate-bn` (BN254) or similar for `bls12_381` (BLS12-381).

See the [benchmarks](/benchmarks/) page for directly-measured numbers on the Raspberry Pi Pico 2 W.
