# 2026-04-27 — RV32 Groth16 verify cost breakdown (BN254)

RV32 counterpart to the M33 breakdown run. Same question, same method, same self-consistency check.

## Headline

**Miller loop: 65.8%, final exp: 33.9%, vk_x: 0.15%. Sum = 99.96%. The cost model is identical on both ISAs — only the absolute times change.**

## The breakdown

| sub-operation | µs | % of verify (1106 ms) |
|---|---:|---:|
| multi-Miller loop (4 pairs) | 728,361 | 65.8% |
| final exponentiation | 375,651 | 33.9% |
| vk_x, tiny scalar (y=9) | 1,656 | 0.15% |
| **sum** | **1,105,668** | **99.9%** |
| actual `groth16_verify` | 1,106,137 | 100% |
| residual | 469 | 0.04% |

The proportions are nearly identical to M33 (67.1% / 32.6% / 0.15%). The cost structure is ISA-independent — it's determined by the Groth16 algorithm, not the processor.

## Cross-ISA breakdown comparison

| sub-operation | M33 (ms) | RV32 (ms) | ratio |
|---|---:|---:|---:|
| miller_loop_4pair | 369.5 | 728.4 | 1.97× |
| final_exp | 179.7 | 375.7 | 2.09× |
| vk_x tiny | 0.84 | 1.66 | 1.97× |
| vk_x full (254-bit) | 30.9 | 70.3 | 2.27× |
| verify total (square) | 550.6 | 1106.1 | 2.01× |

The M33 advantage narrows to ~2.0× in this build pair (vs the ~2.18× from the dedicated poseidon runs). Both are real numbers — build-to-build variation of a few percent is normal for XIP flash code where binary size affects cache line placement. The ratios are internally consistent: miller loop, final exp, and g1 mul all cluster around 2.0×.

The 2.27× for vk_x full (G1 scalar mul with 254-bit scalar) is slightly above average, wich makes sense: the scalar mul path is dominated by Fq mul, and Fq mul is where UMAAL asm helps most. A longer scalar means more Fq mul calls relative to other overhead, so the asm advantage is more visible.

## vk_x scalar cost, cross-ISA

| scalar | M33 (µs) | RV32 (µs) | ratio |
|---|---:|---:|---:|
| y=9 (4-bit) | 844 | 1,656 | 1.96× |
| Merkle root (254-bit) | 30,883 | 70,318 | 2.28× |
| difference | 30,039 | 68,662 | 2.29× |

The per-bit cost of the scalar mul is what scales. The fixed overhead (affine add, projective conversion) doesn't scale with bit count, so as the scalar gets larger the UMAAL advantage becomes more visible.

## Pairing batch math

Single `pairing(G1, G2)` = 624,222 µs.
1 miller loop alone ≈ 624,222 - 375,651 = 248,571 µs.
4-pair multi-Miller loop = 728,361 µs = 2.93× a single one.

Same batching efficiency as M33: ~3× cost for 4× work. The batching savings is ~(4×248.6 - 728.4) = ~266 ms vs four individual pairings.

## Note on verify numbers vs prior rv32-poseidon run

This firmware compiled to a different binary than the 2026-04-27-rv32-poseidon run (added miller_loop_batch + breakdown code; LTO reoptimized the entire binary). The square verify in this run is 1,106 ms vs 1,174 ms in the poseidon run — a 6% difference attributable to code layout changes affecting XIP cache behavior. The breakdown measurements are internally consistent within this run; cross-run absolute comparisons should use the respective run's numbers.
