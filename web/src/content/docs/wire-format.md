---
title: Wire formats
description: EIP-197 (BN254) and EIP-2537 (BLS12-381) binary encodings used by zkmcu's parsers.
---

zkmcu uses Ethereum's two canonical Groth16 wire formats: [EIP-197](https://eips.ethereum.org/EIPS/eip-197) for BN254 and [EIP-2537](https://eips.ethereum.org/EIPS/eip-2537) for BLS12-381. Any Ethereum-compatible Groth16 prover emits bytes one of the two verifier crates accepts without translation.

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

zkmcu enforces strict canonical encoding — values ≥ the respective modulus are rejected with `Error::InvalidFq` / `Error::InvalidFr`. This is stricter than `substrate-bn`'s default, which silently reduces `Fr` values mod `r`. See [security](/security/) for why this matters for nullifier-style applications.

### Points

| Type | Size | Layout |
|------|------|--------|
| `G1` | 64 bytes | `x ‖ y` — two `Fq` coordinates, each 32 BE bytes |
| `G2` | 128 bytes | `x.c1 ‖ x.c0 ‖ y.c1 ‖ y.c0` — four `Fq` coordinates |

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

This pad check closes a malleability gap — without it, an attacker could flip bits in the pad region and the proof would still decode to the same curve point.

### Points

| Type | Size | Layout |
|------|------|--------|
| `G1` | 128 bytes | `x ‖ y` — two `Fp` coordinates, 64 B each |
| `G2` | 256 bytes | `x.c0 ‖ x.c1 ‖ y.c0 ‖ y.c1` — four `Fp` coordinates, 64 B each |

EIP-2537's Fp2 order is `(c0, c1)` — **opposite of EIP-197's BN254 convention**. If you're reading bytes produced by snarkjs or some non-Ethereum BLS12 stack and getting parse failures, Fp2 order is the first thing to check.

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

Field elements are **big-endian** on both curves (matching Ethereum precompile conventions). Length prefixes (`num_ic`, `count`) are **little-endian `u32`**. The `u32` length prefix gives 4 GB of headroom against realistic input sizes, but the parsers always bound-check against the real buffer length before trusting it — see [security](/security/#dos-via-unbounded-allocation).

## Porting between the two

Common pitfalls when adapting code from `zkmcu-verifier` to `zkmcu-verifier-bls12`:

1. **Fp2 order flip**: `(c1, c0)` on BN254 vs `(c0, c1)` on BLS12-381. Most integration bugs land here.
2. **Fp size**: 32 bytes on BN254 vs 64 bytes on BLS12-381. Every size constant doubles, but the 16-byte padding is unique to EIP-2537.
3. **Identity encoding**: all-zero bytes on both, so this one transfers directly.
4. **`Fr` size**: both curves use a 32-byte Fr. Scalar fields are within a bit of each other (255-bit BLS12 vs 254-bit BN254) — no format changes needed for public inputs.
