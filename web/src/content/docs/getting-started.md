---
title: Getting started
description: Add zkmcu to a Rust project and verify your first proof.
---

zkmcu is a `no_std` Rust library. It runs on host targets (for testing) and on embedded targets (its main job) from the same source tree.

## Add to your `Cargo.toml`

```toml
[dependencies]
zkmcu-verifier = "0.1"
```

On an embedded target you also need a global allocator. `embedded-alloc` works well:

```toml
embedded-alloc = { version = "0.7", features = ["llff"] }
```

## Verify a proof

Simplest case — you have three byte buffers (verifying key, proof, public inputs) in EIP-197 format:

```rust
let ok = zkmcu_verifier::verify_bytes(&vk_bytes, &proof_bytes, &public_bytes)?;
assert!(ok);
```

That's the whole API for one-shot verification. See the [wire format](/wire-format/) page for what those buffers must look like.

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

## Generating test proofs

For development you need some proofs to verify against. The `zkmcu-host-gen` binary produces EIP-197-format test vectors using `arkworks`:

```bash
cargo run -p zkmcu-host-gen --release
```

This writes `crates/zkmcu-vectors/data/<name>/{vk,proof,public}.bin`. Two vectors ship by default: `square` (one public input) and `squares-5` (five public inputs). For production use, generate proofs with whatever prover your application uses — any Ethereum-compatible BN254 Groth16 prover works.

## On an embedded target

See the reference firmware at [`crates/bench-rp2350-m33/`](https://github.com/Niek-Kamer/zkmcu/tree/main/crates/bench-rp2350-m33) for a complete working example targeting the RP2350 Cortex-M33 core. It brings up clocks, initialises a heap, parses baked-in test vectors via `include_bytes!`, runs `verify`, and prints results over USB-CDC serial.

The Hazard3 RV32 equivalent lives at [`crates/bench-rp2350-rv32/`](https://github.com/Niek-Kamer/zkmcu/tree/main/crates/bench-rp2350-rv32). Same source for the crypto, different linker script and cycle-counter read.

## What the target needs

- `no_std` Rust toolchain (stable, `rustc` 1.82 or newer)
- A global allocator
- **112 KB of SRAM** during verification (96 KB heap + 16 KB peak stack + small statics). Any 128 KB SRAM-class MCU works — nRF52832, STM32F405, Ledger ST33, Infineon SLE78.
- About **75 KB of flash** for the verifier + substrate-bn backend.

See the [benchmarks](/benchmarks/) page for directly-measured numbers.
