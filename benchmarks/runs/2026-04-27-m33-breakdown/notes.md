# 2026-04-27 — M33 Groth16 verify cost breakdown (BN254)

Now we know not just what Groth16 costs, but why. The verify splits into three measurable sub-operations and we can read exactly where the time goes.

## Headline

**The multi-Miller loop is 67% of verify. Final exp is 33%. The vk_x scalar mul is 0.15% for a tiny scalar and 5.6% for a full 254-bit one. Everything adds up to 99.9%.**

## The breakdown

| sub-operation | µs | % of verify (550 ms) |
|---|---:|---:|
| multi-Miller loop (4 pairs) | 369,513 | 67.1% |
| final exponentiation | 179,653 | 32.6% |
| vk_x, tiny scalar (y=9) | 844 | 0.15% |
| **sum** | **550,010** | **99.9%** |
| actual `groth16_verify` | 550,646 | 100% |
| residual (other G1 adds, GT eq check) | 636 | 0.1% |

The residual 636 µs is genuinely everything else in the verifier: the remaining G1 point additions in `vk_x`, the `Gt` equality check at the end, stack/allocation overhead. None of it matters.

## What changes when the public input is a full 254-bit scalar

For the square circuit `public[0] = 9` — a 4-bit scalar with 2 set bits. The vk_x mul costs 844 µs.

For the Poseidon circuit `public[0]` is a Merkle root: a full 254-bit field element with ~127 set bits on average. The vk_x mul costs 30,883 µs — 36× more. That's exactly the 29 ms delta we see between square (550 ms) and poseidon (579 ms) verify.

## The pairing batch math checks out

Single `pairing(G1, G2)` = 306,895 µs.
That's 1 miller loop + 1 final exp.
So 1 miller loop alone ≈ 306,895 - 179,653 = 127,242 µs.
4-pair multi-Miller loop = 369,513 µs = 2.9× a single one.

Batching 4 pairs costs only 2.9× instead of 4× — about 139 ms saved vs running 4 individual pairings. That's the whole point of `pairing_batch`.

## The cost model, fully quantified

```
verify ≈ miller_loop(4 pairs) + final_exp + Σ vk_x[i]
       ≈ 369.5 ms + 179.7 ms + n × cost(scalar_i)
```

where `cost(scalar_i)` ranges from ~0.85 ms (4-bit) to ~31 ms (254-bit). The constraint count of the circuit plays zero role.

## Cross-ISA prediction

Everything scales by the same 2.18× factor (pure UMAAL asm advantage in field mul):

| sub-operation | M33 (ms) | predicted RV32 (ms) |
|---|---:|---:|
| miller_loop_4pair | 369.5 | ~805 |
| final_exp | 179.7 | ~392 |
| vk_x_tiny | 0.84 | ~1.8 |
| vk_x_full | 30.9 | ~67 |
| total verify (square) | 550.6 | ~1,200 |
| total verify (poseidon) | 579.4 | ~1,263 |

The actual RV32 square was 1,174 ms and poseidon d3 was 1,241 ms — both match the prediction within 3%.
