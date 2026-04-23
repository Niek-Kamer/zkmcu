---
title: Getting started
description: Add zkmcu to a Rust project and verify your first proof.
---

zkmcu is a family of `no_std` Rust libraries. Three verifier crates, one per supported proof system. Runs on host targets (for testing) and on embedded targets (its main job) from the same source tree.

## Pick a proof system

| Proof system | Crate | Wire format | Best for | Proof size |
|---|---|---|---|---:|
| **BN254 Groth16** | `zkmcu-verifier` | EIP-197 | Ethereum + all EVM L2s, Semaphore, Tornado Cash, MACI | 256 B |
| **BLS12-381 Groth16** | `zkmcu-verifier-bls12` | EIP-2537 | Ethereum sync-committee, Zcash, Filecoin, Aleo | 512 B |
| **Winterfell STARK** | `zkmcu-verifier-stark` | winterfell 0.13 | Fast verify, post-quantum, larger proofs OK | 25-31 KB |

Not sure which? Quick decision guide:

- If the proof lives in an Ethereum mainnet transaction or Semaphore-style application → **BN254 Groth16**
- If it's for Ethereum sync-committee, Zcash, Filecoin PoSt / PoRep, or Aleo → **BLS12-381 Groth16**
- If verify needs to happen in under 100 ms, or the deployment is post-quantum-sensitive → **Winterfell STARK**
- If the transport is bandwidth-bound (LoRa, NFC) → Groth16 (256-512 B proofs fit a single radio frame, STARK's 30 KB doesn't)

## Add to your `Cargo.toml`

```toml
[dependencies]
zkmcu-verifier       = "0.1"       # BN254 Groth16
zkmcu-verifier-bls12 = "0.1"       # BLS12-381 Groth16
zkmcu-verifier-stark = "0.1"       # Winterfell STARK
```

On an embedded target you also need a global allocator. Recommended choice depends on the proof system:

```toml
# Default choice: TlsfHeap is O(1) deterministic and fits the 128 KB tier.
embedded-alloc = { version = "0.7", features = ["tlsf"] }

# Alternative: LlffHeap (linked-list first-fit). Slightly faster median verify
# on Cortex-M33 for Groth16 paths; noisier variance on STARK paths.
# embedded-alloc = { version = "0.7", features = ["llff"] }
```

For STARK specifically, the allocator choice has a measurable effect on timing variance, see [Deterministic timing](/determinism/). For Groth16, either works fine.

## Verify a Groth16 proof

Simplest case, you have three byte buffers (verifying key, proof, public inputs) in the curve's wire format:

```rust
let ok = zkmcu_verifier::verify_bytes(&vk_bytes, &proof_bytes, &public_bytes)?;
// or the BLS12 equivalent:
let ok = zkmcu_verifier_bls12::verify_bytes(&vk_bytes, &proof_bytes, &public_bytes)?;

assert!(ok);
```

See [wire formats](/wire-format/) for what those buffers must look like on each curve.

## Verify a STARK proof

STARK verify doesn't have a separate VK, the AIR definition is the verifier-side invariant. You compile your AIR into the verifier binary. For the reference Fibonacci AIR that ships with `zkmcu-verifier-stark`:

```rust
use zkmcu_verifier_stark::{parse_proof, fibonacci};

let proof  = parse_proof(&proof_bytes)?;
let public = fibonacci::parse_public(&public_bytes)?;
fibonacci::verify(proof, public)?;
```

For a custom AIR: implement winterfell's `Air` trait for your transition constraints, then call `winterfell::verify::<YourAir, Blake3_256<BaseElement>, DefaultRandomCoin<Blake3_256<BaseElement>>, MerkleTree<Blake3_256<BaseElement>>>(...)` with a `MinConjecturedSecurity` threshold you pick. The Fibonacci variant in this crate is a thin wrapper around exactly that call, copy its shape.

## Verify many proofs against one VK (Groth16)

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

Identical shape for `zkmcu_verifier_bls12`. STARK doesn't need this step (no VK).

## Generating test proofs

For development you need some proofs to verify against. The `zkmcu-host-gen` binary produces wire-format test vectors:

```bash
cargo run -p zkmcu-host-gen --release                # all systems, default vectors
cargo run -p zkmcu-host-gen --release -- bn254       # BN254 only
cargo run -p zkmcu-host-gen --release -- bls12-381   # BLS12-381 only
cargo run -p zkmcu-host-gen --release -- stark       # Winterfell Fibonacci STARK
```

Writes `crates/zkmcu-vectors/data/<name>/{vk,proof,public}.bin` (STARK has no `vk.bin`, AIR is the invariant). Vectors ship by default:

- `square`, 1 public input (BN254 + BLS12-381, two copies of the same circuit)
- `squares-5`, 5 public inputs, small-scalar (BN254 + BLS12-381)
- `semaphore-depth-10`, **real** Semaphore Groth16 proof, 4 public inputs, BN254 only. See [Semaphore page](/semaphore/) for the generator pipeline
- `stark-fib-1024`, Fibonacci STARK at `FieldExtension::Quadratic`, 95-bit conjectured security

For production use, generate proofs with whatever prover your application uses. Any Ethereum-compatible BN254 Groth16 prover emits EIP-197 bytes; any BLS12-381 Groth16 prover emits EIP-2537 bytes; any winterfell-based prover emits proof bytes directly consumable by `zkmcu-verifier-stark` as long as the AIR definition agrees.

## On an embedded target

Six reference firmware crates ship in the repo. Pick the one that matches your proof system + ISA:

| Proof system | Cortex-M33 | Hazard3 RV32 |
|---|---|---|
| BN254 Groth16 | [`bench-rp2350-m33`](https://github.com/Niek-Kamer/zkmcu/tree/main/crates/bench-rp2350-m33) | [`bench-rp2350-rv32`](https://github.com/Niek-Kamer/zkmcu/tree/main/crates/bench-rp2350-rv32) |
| BLS12-381 Groth16 | [`bench-rp2350-m33-bls12`](https://github.com/Niek-Kamer/zkmcu/tree/main/crates/bench-rp2350-m33-bls12) | [`bench-rp2350-rv32-bls12`](https://github.com/Niek-Kamer/zkmcu/tree/main/crates/bench-rp2350-rv32-bls12) |
| STARK | [`bench-rp2350-m33-stark`](https://github.com/Niek-Kamer/zkmcu/tree/main/crates/bench-rp2350-m33-stark) | [`bench-rp2350-rv32-stark`](https://github.com/Niek-Kamer/zkmcu/tree/main/crates/bench-rp2350-rv32-stark) |

Each brings up clocks, initialises a heap, parses baked-in test vectors via `include_bytes!`, runs `verify`, and prints results over USB-CDC serial. Same source for the crypto, different linker scripts and cycle-counter reads per ISA.

## What the target needs

- `no_std` Rust toolchain (stable, `rustc` 1.82 or newer)
- A global allocator (TlsfHeap or LlffHeap, see table above)
- **About 100 KB of SRAM during verify** for any of the three systems on Cortex-M33. All three fit on any 128 KB SRAM-class MCU, nRF52832, STM32F405, Ledger ST33, Infineon SLE78.
- **About 75-200 KB of flash** depending on the proof system (BN254 is lightest, winterfell is heaviest due to the multiple winter-* sub-crates pulled in).

See the [benchmarks](/benchmarks/) page for directly-measured numbers on the Raspberry Pi Pico 2 W.
