---
title: Getting started
description: Add zkmcu to a Rust project and verify your first proof.
---

Yeah so zkmcu is a family of `no_std` Rust libraries. Three verifier crates, one per proof system. Runs on host for tests, runs on embedded for the actual job, same source either way.

## Pick a proof system

| Proof system | Crate | Wire format | Best for | Proof size |
|---|---|---|---|---:|
| **BN254 Groth16** | `zkmcu-verifier` | EIP-197 | Ethereum + all EVM L2s, Semaphore, Tornado Cash, MACI | 256 B |
| **BLS12-381 Groth16** | `zkmcu-verifier-bls12` | EIP-2537 | Ethereum sync-committee, Zcash, Filecoin, Aleo | 512 B |
| **Winterfell STARK** | `zkmcu-verifier-stark` | winterfell 0.13 | Fast verify, post-quantum, larger proofs OK | 25-31 KB |

Not sure wich? Quick picker:

- If the proof lives in an Ethereum mainnet transaction or a Semaphore-style application → **BN254 Groth16**
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

On embedded you also need a global allocator. The right pick depends on wich proof system you use:

```toml
# Default: TlsfHeap is O(1) deterministic and fits the 128 KB tier.
embedded-alloc = { version = "0.7", features = ["tlsf"] }

# Alternative: LlffHeap (linked-list first-fit). Slightly faster median verify
# on Cortex-M33 for Groth16; noisier variance on STARK paths.
# embedded-alloc = { version = "0.7", features = ["llff"] }
```

For STARK the allocator choice actually moves timing variance by a lot, see [Deterministic timing](/determinism/). For Groth16 either one is fine because the verify path barely allocates.

## Verify a Groth16 proof

Simplest shape. You have three byte buffers (vk, proof, public inputs) already in the curve's wire format, you call `verify_bytes`, you get a bool back:

```rust
let ok = zkmcu_verifier::verify_bytes(&vk_bytes, &proof_bytes, &public_bytes)?;
// or the BLS12 equivalent:
let ok = zkmcu_verifier_bls12::verify_bytes(&vk_bytes, &proof_bytes, &public_bytes)?;

assert!(ok);
```

See [wire formats](/wire-format/) for what the bytes actually look like per curve, byte for byte.

## Verify a STARK proof

STARK is different. There is no VK, the AIR definition itself is the verifier-side invariant. So you compile your AIR into the verifier binary and go. For the reference Fibonacci AIR that ships with `zkmcu-verifier-stark`:

```rust
use zkmcu_verifier_stark::{parse_proof, fibonacci};

let proof  = parse_proof(&proof_bytes)?;
let public = fibonacci::parse_public(&public_bytes)?;
fibonacci::verify(proof, public)?;
```

For a custom AIR: implement winterfell's `Air` trait for your transition constraints, then call `winterfell::verify::<YourAir, Blake3_256<BaseElement>, DefaultRandomCoin<Blake3_256<BaseElement>>, MerkleTree<Blake3_256<BaseElement>>>(...)` with a `MinConjecturedSecurity` threshold you pick yourself. The Fibonacci variant in this crate is a thin wrapper around exactly that call, so copy its shape and swap `FibAir` for yours.

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

Identical shape for `zkmcu_verifier_bls12`. STARK doesn't need this step because there's no VK to parse.

## Generating test proofs

For development you'll need proofs to verify against. The `zkmcu-host-gen` binary churns out wire-format test vectors:

```bash
cargo run -p zkmcu-host-gen --release                # all systems, default vectors
cargo run -p zkmcu-host-gen --release -- bn254       # BN254 only
cargo run -p zkmcu-host-gen --release -- bls12-381   # BLS12-381 only
cargo run -p zkmcu-host-gen --release -- stark       # Winterfell Fibonacci STARK
```

Writes `crates/zkmcu-vectors/data/<name>/{vk,proof,public}.bin` (STARK has no `vk.bin`, the AIR is the invariant). Vectors that ship by default:

- `square`, 1 public input (BN254 + BLS12-381, two copies of the same circuit)
- `squares-5`, 5 public inputs, small-scalar (BN254 + BLS12-381)
- `semaphore-depth-10`, **real** Semaphore Groth16 proof, 4 public inputs, BN254 only. See [Semaphore page](/semaphore/) for the generator pipeline
- `stark-fib-1024`, Fibonacci STARK at `FieldExtension::Quadratic`, 95-bit conjectured security

For production use, generate proofs with wichever prover your application already uses. Any Ethereum-compatible BN254 Groth16 prover outputs EIP-197 bytes, any BLS12-381 prover outputs EIP-2537, any winterfell-based prover outputs bytes that `zkmcu-verifier-stark` reads directly (as long as your AIR definition matches on both sides).

## On an embedded target

Six reference firmware crates ship in the repo. Pick wichever matches your proof system + ISA combo:

| Proof system | Cortex-M33 | Hazard3 RV32 |
|---|---|---|
| BN254 Groth16 | [`bench-rp2350-m33`](https://github.com/Niek-Kamer/zkmcu/tree/main/crates/bench-rp2350-m33) | [`bench-rp2350-rv32`](https://github.com/Niek-Kamer/zkmcu/tree/main/crates/bench-rp2350-rv32) |
| BLS12-381 Groth16 | [`bench-rp2350-m33-bls12`](https://github.com/Niek-Kamer/zkmcu/tree/main/crates/bench-rp2350-m33-bls12) | [`bench-rp2350-rv32-bls12`](https://github.com/Niek-Kamer/zkmcu/tree/main/crates/bench-rp2350-rv32-bls12) |
| STARK | [`bench-rp2350-m33-stark`](https://github.com/Niek-Kamer/zkmcu/tree/main/crates/bench-rp2350-m33-stark) | [`bench-rp2350-rv32-stark`](https://github.com/Niek-Kamer/zkmcu/tree/main/crates/bench-rp2350-rv32-stark) |

Each one brings up the clocks, inits a heap, parses baked-in test vectors via `include_bytes!`, runs `verify`, and spits cycle counts over USB-CDC serial. Crypto source is byte-for-byte the same across firmware, only linker scripts and cycle-counter reads change per ISA.

## What the target needs

- `no_std` Rust toolchain (stable, `rustc` 1.82 or newer)
- A global allocator (TlsfHeap or LlffHeap, see table above)
- **About 100 KB of SRAM during verify** for any of the three systems on Cortex-M33. All three fit on any 128 KB SRAM-class MCU, so nRF52832, STM32F405, Ledger ST33, Infineon SLE78 all work
- **About 75-200 KB of flash** depending on the proof system (BN254 is lightest, winterfell is heaviest because it pulls in multiple `winter-*` sub-crates)

See [benchmarks](/benchmarks/) for the actual measured numbers on a $7 Pico 2 W.
