# 2026-04-24 — RV32 BLS12-381 Groth16, bench-core rebaseline

First RV32 BLS12 run with `heap_peak` measured. Same zkcrypto versions
as the pre-refactor baseline, just built against `bench-core` and
getting `TrackingLlff` for free.

## Headline

**Groth16 verify (x^2 = y, BLS12-381): 5195 ms median, +0.87 %
vs 5150 ms pre-refactor. squares-5: 10907 ms, +0.34 %.**

Tight. TrackingLlff atomics overhead is much smaller here than on
the RV32 BN run (+2.7 %) because zkcrypto's bls12_381 does fewer
inner-loop allocations than substrate-bn — it leans on stack-allocated
fixed-size field elements wherever possible.

## Heap confirmation

`heap_peak = 79_360 B` — **byte-identical to M33 BLS12**. Predicted
~130-160 KB based on theory + pairing tower structure; actual is half
that. Probably zkcrypto is doing smart in-place reuse of Fq2/Fq6
allocations that the theory didn't account for.

First time this is measured on RV32, so no prior baseline for
comparison on heap specifically.

## g2mul delta

g2mul went 1002 → 1041 ms (+3.9 %). Interestingly that's the
*opposite* sign from M33 BLS12 g2mul (523 → 502, −3.9 %). Possible
explanations:

- TrackingLlff overhead is hitting the Fq2 mul hot loop specifically
  on RV32 because Hazard3 doesn't have UMULL-pipelined multiplications,
  so per-cycle cost is higher.
- Register allocation in zkcrypto's Fq2 mul shifted unfavorably on
  RV32 but favorably on M33. RV32 has 31 GPRs so less room for
  movement one way or the other, but +3.9 % isn't that big either.

Honestly idk, the BLS12 verify-total delta is only +0.9 % so the
g2mul shift is noise in the big picture.

## Cross-ISA reference

M33 BLS12 sibling (same commit) lands at 1998 ms. RV32/M33 ratio =
2.60x. Bigger than the BN ratio (2.12x), mostly because BLS12-381's
381-bit base field hurts Hazard3 more than BN254's 254-bit does.
