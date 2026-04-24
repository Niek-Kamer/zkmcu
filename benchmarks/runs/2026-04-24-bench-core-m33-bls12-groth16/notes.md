# 2026-04-24 — M33 BLS12-381 Groth16, bench-core rebaseline

Same silicon and same zkcrypto versions as `2026-04-22-m33-bls12-baseline`,
just rebuilt after the `bench-core` refactor (commit `98c7af5`).

## Headline

**Groth16 verify (x^2 = y, BLS12-381): 1997 ms median, −0.84 %
vs the 2014 ms pre-refactor baseline. Within noise.**

## The interesting one: g2mul

g2mul dropped 523 → 502 ms (−3.94 %). That's outside run-to-run
noise range for this op. Best guess is the same thing we saw on M33
BabyBear STARK: the closure-wrapping changed how captures survive
across the measurement boundary and LLVM gave the Fq2 mul hot loop
slightly better register allocation. BLS12 G2 arithmetic is Fq2-heavy
(each G2 op does a handful of Fq2 mul/add/sub) and Thumb-2's 13-ish
usable GPRs is right at the edge for this. Goldilocks would be the
opposite experiment but obviously not applicable here.

g1mul and pairing moved less than 1 %, wich is the more typical
response since those code paths are better-inlined at baseline.

## Everything else

heap_peak 79360 B, stack 19360 B, both byte-identical to the 04-22
baseline. Result `ok=true` on every sample. No boot-line truncation
on the serial output this time, probably because the 2 s BLS12 verify
is long enough that the host fully drains the IN endpoint each round.

## Cross-ISA reference

RV32 BLS12 sibling (same commit, same bench-core) came in at 5195 ms,
heap_peak 79360 B identical to this run. First time the BLS12 heap
footprint is confirmed byte-identical across ISAs.
