---
title: Wire format
description: EIP-197-compatible binary encoding used by zkmcu's parsers.
---

zkmcu uses the same binary encoding as Ethereum's precompiled pairing contract ([EIP-197](https://eips.ethereum.org/EIPS/eip-197)). Any Ethereum-compatible BN254 Groth16 prover produces bytes that zkmcu verifies without additional translation.

## Field elements

| Type | Size | Encoding |
|------|------|----------|
| `Fq` | 32 bytes | Big-endian unsigned integer, strictly less than the BN254 base modulus `p` |
| `Fr` | 32 bytes | Big-endian unsigned integer, strictly less than the BN254 scalar modulus `r` |

zkmcu enforces strict canonical encoding — values ≥ the respective modulus are rejected with `Error::InvalidFq` / `Error::InvalidFr`. This is stricter than `substrate-bn`'s default, which silently reduces `Fr` values mod `r`. See [security](/security/) for why this matters for nullifier-style applications.

## Points

| Type | Size | Layout |
|------|------|--------|
| `G1` | 64 bytes | `x ‖ y` — two `Fq` coordinates, each 32 BE bytes |
| `G2` | 128 bytes | `x.c1 ‖ x.c0 ‖ y.c1 ‖ y.c0` — four `Fq` coordinates |

The point at infinity is encoded as all-zero bytes (`[0u8; 64]` for `G1`, `[0u8; 128]` for `G2`). Parsers accept this encoding. The verifier rejects proofs that exploit identity points.

Points that are not on the curve (or not on the twist for `G2`) are rejected by `substrate-bn`'s `AffineG1::new` / `AffineG2::new` constructors, surfaced as `Error::InvalidG1` / `Error::InvalidG2`.

## Verifying key

```text
alpha(G1) ‖ beta(G2) ‖ gamma(G2) ‖ delta(G2) ‖ num_ic(u32 LE) ‖ ic[num_ic](G1)
```

Total size = `64 + 3·128 + 4 + num_ic·64` bytes.

| Circuit | `num_ic` | VK size |
|---|---:|---:|
| 1 public input (`square`) | 2 | 580 B |
| 5 public inputs (`squares-5`) | 6 | 836 B |
| 50 public inputs | 51 | 3,716 B |

`num_ic` is bounds-checked against the actual buffer length with checked arithmetic before any allocation — see [security](/security/#dos-via-unbounded-allocation-fixed).

## Proof

```text
A(G1) ‖ B(G2) ‖ C(G1)
```

Always exactly 256 bytes. Groth16 proof size is constant regardless of circuit size.

## Public inputs

```text
count(u32 LE) ‖ input[count](Fr)
```

Total size = `4 + count · 32` bytes. Same bounds-check discipline as `num_ic`.

## Endianness notes

Field elements are **big-endian**. Length prefixes (`num_ic`, `count`) are **little-endian `u32`**. This matches the conventions Ethereum client implementations use when serialising to the pairing precompile. The `u32` length prefix gives 4 GB of headroom against realistic input sizes — but the parsers always bound-check against the real buffer length before trusting it.
