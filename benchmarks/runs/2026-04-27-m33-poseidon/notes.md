# 2026-04-27 — M33 Groth16 Poseidon Merkle membership (BN254)

So the Poseidon result answers the question wich the square/squares-5/semaphore benchmarks never directly addressed: does verify cost scale with circuit complexity? The answer is no, and now we have the hardware numbers to prove it.

## Headline

**Depth-3 (739 constraints) and depth-10 (2461 constraints) both verify in 570 ms. Delta is 537 µs — 0.09%. The verifier literally doesn't notice the 3.3× increase in circuit complexity.**

## Full picture

| circuit | constraints | public inputs | ic_size | verify (ms) | Δ vs square |
|---|---:|---:|---:|---:|---:|
| square | 1 | 1 (y=9, tiny) | 2 | 541 | — |
| squares-5 | 5 | 5 (small) | 6 | 547 | +6 ms |
| poseidon depth-3 | 739 | 1 (254-bit root) | 2 | 570 | +29 ms |
| poseidon depth-10 | 2461 | 1 (254-bit root) | 2 | 570 | +29 ms |
| semaphore depth-10 | — | 4 (254-bit each) | 5 | 660 | +119 ms |

The cost model is clear: verify time depends on `ic_size` (the number of public inputs + 1), not constraint count. And within ic_size, it depends on the Hamming weight of the public input scalars.

## Why poseidon costs 29 ms more than square despite both having ic_size=2

The `vk_x` computation is `IC[0] + Σ public[i] * IC[i+1]`. For the square circuit `public[0] = 9`, wich is a 4-bit scalar with 2 set bits — the G1 scalar mul is nearly free. For poseidon, `public[0]` is a Merkle root: a full 254-bit field element with ~127 set bits on average. That's a typical G1 scalar mul (~33 ms). You can see the same effect in the `g1_mul` rows in this run: iter 2 and 7 came in at 17 ms (low-Hamming scalar) vs iters 1, 3–6, 9–10 at ~33 ms (typical Hamming weight).

So the 29 ms difference between square (tiny public input) and poseidon (full 254-bit public input) is entirely explained by the `vk_x` scalar multiplication. Nothing circuit-specific.

## What this adds to the project

The previous benchmarks established what it costs to verify. This one explains why. The practical rule for circuit designers targeting embedded hardware: minimize public input count and avoid unnecessarily large scalars. Circuit size is basically free from the verifier's perspective.

## Footprint

| metric | value |
|---|---|
| `.text` | 101,264 B |
| heap peak | 82,336 B (verified, same as previous M33 runs) |
| stack peak | 15,724 B |

Text grew vs the 2026-04-23 run (73,632 B) because two Poseidon vector blobs are now included in flash via `include_bytes!`.
