---
title: Wire formats
description: EIP-197 (BN254), EIP-2537 (BLS12-381), and winterfell 0.13 (STARK) binary encodings used by zkmcu's parsers.
---

zkmcu uses three wire formats, one per supported proof system:

- **[EIP-197](https://eips.ethereum.org/EIPS/eip-197)** for BN254 Groth16, Ethereum's canonical precompile format
- **[EIP-2537](https://eips.ethereum.org/EIPS/eip-2537)** for BLS12-381 Groth16, Ethereum's newer BLS precompile format
- **winterfell 0.13 `Proof::to_bytes()`** for Goldilocks STARKs, winterfell's own serialisation

Any Ethereum-compatible Groth16 prover emits bytes one of the two Groth16 verifier crates accepts without translation. Any winterfell-based STARK prover emits bytes `zkmcu-verifier-stark::parse_proof` accepts, provided the prover and verifier agree on the AIR definition.

## Summary

| | BN254 / EIP-197 | BLS12-381 / EIP-2537 |
|---|---|---|
| `Fq` / `Fp` | 32 B big-endian | **64 B** (16 zero pad + 48 BE) |
| `Fr` | 32 B big-endian | 32 B big-endian |
| `G1` | 64 B | 128 B |
| `G2` | 128 B | 256 B |
| **`G2` Fp2 order** | **`(c1, c0)`** | **`(c0, c1)`** |
| Proof total | 256 B | 512 B |
| Identity point | all-zero bytes | all-zero bytes |

The **Fp2 byte order flip** is the most common source of integration bugs when porting between the two. If a proof verifies through arkworks but fails through zkmcu, the `G2` bytes are the first place to look.

## BN254 / EIP-197

### Field elements

| Type | Size | Encoding |
|------|------|----------|
| `Fq` | 32 bytes | Big-endian unsigned integer, strictly less than the BN254 base modulus `p` |
| `Fr` | 32 bytes | Big-endian unsigned integer, strictly less than the BN254 scalar modulus `r` |

zkmcu enforces strict canonical encoding, values ≥ the respective modulus are rejected with `Error::InvalidFq` / `Error::InvalidFr`. This is stricter than `substrate-bn`'s default, which silently reduces `Fr` values mod `r`. See [security](/security/) for why this matters for nullifier-style applications.

### Points

| Type | Size | Layout |
|------|------|--------|
| `G1` | 64 bytes | `x ‖ y`, two `Fq` coordinates, each 32 BE bytes |
| `G2` | 128 bytes | `x.c1 ‖ x.c0 ‖ y.c1 ‖ y.c0`, four `Fq` coordinates |

Note the Fp2 order: BN254's convention is `(c1, c0)` because Ethereum's original BN128 precompile shipped with that order and the rest of the ecosystem followed.

### Verifying key

```text
alpha(G1) ‖ beta(G2) ‖ gamma(G2) ‖ delta(G2) ‖ num_ic(u32 LE) ‖ ic[num_ic](G1)
```

Total size = `64 + 3·128 + 4 + num_ic·64` bytes.

| Circuit | `num_ic` | VK size |
|---|---:|---:|
| 1 public input (`square`) | 2 | 580 B |
| 4 public inputs (Semaphore depth-10) | 5 | 772 B |
| 5 public inputs (`squares-5`) | 6 | 836 B |
| 50 public inputs | 51 | 3,716 B |

### Proof + public inputs

```text
A(G1) ‖ B(G2) ‖ C(G1)                         ← 256 B proof, constant size
count(u32 LE) ‖ input[count](Fr)              ← public inputs
```

## BLS12-381 / EIP-2537

### Field elements

| Type | Size | Encoding |
|------|------|----------|
| `Fp` | 64 bytes | **16 leading zero bytes**, then 48-byte big-endian integer, strictly less than the BLS12-381 base modulus |
| `Fr` | 32 bytes | Big-endian, strictly less than the BLS12-381 scalar modulus |

The 16-byte padding comes from EIP-2537's alignment choice: BLS12-381's 381-bit base field fits in 48 bytes, but Ethereum's precompile ABI uses 32-byte words, so every `Fp` value is left-padded with 16 zeros to land on a 64-byte boundary. zkmcu's parsers **require that padding to be exactly zero**; any non-zero byte in the pad region is rejected as `Error::InvalidFp`.

This pad check closes a malleability gap, without it, an attacker could flip bits in the pad region and the proof would still decode to the same curve point.

### Points

| Type | Size | Layout |
|------|------|--------|
| `G1` | 128 bytes | `x ‖ y`, two `Fp` coordinates, 64 B each |
| `G2` | 256 bytes | `x.c0 ‖ x.c1 ‖ y.c0 ‖ y.c1`, four `Fp` coordinates, 64 B each |

EIP-2537's Fp2 order is `(c0, c1)`, **opposite of EIP-197's BN254 convention**. If you're reading bytes produced by snarkjs or some non-Ethereum BLS12 stack and getting parse failures, Fp2 order is the first thing to check.

Internally, zkcrypto's `bls12_381` crate uses `(c1, c0)` for its own uncompressed G2 encoding, so zkmcu's BLS12 parser does a two-step conversion: EIP-2537 `(c0, c1)` → strip padding → swap to `(c1, c0)` → hand to `G2Affine::from_uncompressed`.

### Verifying key + proof + public inputs

Same container shape as BN254, different point sizes:

```text
alpha(G1) ‖ beta(G2) ‖ gamma(G2) ‖ delta(G2) ‖ num_ic(u32 LE) ‖ ic[num_ic](G1)
A(G1) ‖ B(G2) ‖ C(G1)                         ← 512 B proof, constant size
count(u32 LE) ‖ input[count](Fr)              ← public inputs
```

| Circuit | `num_ic` | VK size |
|---|---:|---:|
| 1 public input (`square`) | 2 | 1,156 B |
| 5 public inputs (`squares-5`) | 6 | 1,668 B |

## Endianness notes

Field elements are **big-endian** on both curves (matching Ethereum precompile conventions). Length prefixes (`num_ic`, `count`) are **little-endian `u32`**. The `u32` length prefix gives 4 GB of headroom against realistic input sizes, but the parsers always bound-check against the real buffer length before trusting it, see [security](/security/#dos-via-unbounded-allocation).

## Winterfell STARK

The STARK wire format is winterfell's own `Proof::to_bytes()` / `Proof::from_bytes()` pair, not an Ethereum-standardised format. It's the serialised form of winterfell's internal `Proof` struct, carrying the trace commitment, constraint commitment, FRI layer commitments, query responses, and out-of-domain evaluations.

### Proof structure (conceptual)

```text
  header + context (~200 B)          ← trace length, AIR metadata, options
  trace Merkle root (32 B)
  constraint Merkle root (32 B)
  OOD trace evaluations              ← base-field or F_{p^2} depending on extension
  OOD constraint evaluations
  FRI layer commitments              ← ~13 roots at blowup 8, trace length 1024
  FRI query proofs                   ← 32 queries × Merkle auth path per layer
  FRI remainder polynomial
```

Total size depends on AIR, trace length, blowup, query count, and field-extension choice. For the reference Fibonacci AIR at $N = 1024$, blowup 8, 32 queries:

| Config | Security | Proof size |
|---|---:|---:|
| `FieldExtension::None` | 63-bit conjectured | 25,332 B |
| `FieldExtension::Quadratic` | **95-bit conjectured** *(production default)* | **30,888 B** |
| `FieldExtension::Cubic` | ~128-bit conjectured | ~40 KB (est.) |

The jump from None to Quadratic only grows the proof by ~22 % because FRI layer evaluations are ~half the proof by weight and doubling them is partly offset by auth-path bytes that stay the same.

### Public inputs

The Fibonacci AIR's public input is a single Goldilocks field element (the claimed result `Fib(2N) mod p`), encoded as 8 bytes little-endian `u64`. For custom AIRs, public-input encoding is whatever the AIR's `ToElements` impl defines, byte-level layout is the application's responsibility.

### No VK

STARK verify doesn't have a verifying key in the Groth16 sense. The AIR definition (transition constraints, boundary assertions, trace width) is the verifier-side invariant, it's compiled into the verifier binary, not passed at runtime. This is why `zkmcu-verifier-stark` has no `parse_vk` function.

## Porting between formats

Common pitfalls when adapting code across verifier crates:

**BN254 ↔ BLS12-381 (Groth16 ↔ Groth16)**:

1. **Fp2 order flip**: `(c1, c0)` on BN254 vs `(c0, c1)` on BLS12-381. Most integration bugs land here.
2. **Fp size**: 32 bytes on BN254 vs 64 bytes on BLS12-381. Every size constant doubles, but the 16-byte padding is unique to EIP-2537.
3. **Identity encoding**: all-zero bytes on both, so this one transfers directly.
4. **`Fr` size**: both curves use a 32-byte Fr. Scalar fields are within a bit of each other (255-bit BLS12 vs 254-bit BN254), no format changes needed for public inputs.

**Groth16 ↔ STARK**:

1. **No VK on STARK side**, the AIR compiled into the verifier binary replaces the role the VK plays for Groth16. This means STARK upgrades require re-flashing firmware; Groth16 upgrades can hot-swap the VK at runtime.
2. **Proof size differs by 50-100×**, 256 B vs 30 KB. If your transport was sized for Groth16 payloads, it will need rework for STARK.
3. **Public-input encoding is AIR-specific for STARK**, the 4-byte LE count + fixed-size Fr scheme used by both Groth16 crates doesn't apply; whatever your AIR's `ToElements` impl says is the format.
