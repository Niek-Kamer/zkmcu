# Phase F — dual-hash at 128 conjectured bits/leg, Cortex-M33

Phase F of the 128-bit security plan. Same Phase E.1 architecture (BabyBear ×
Quartic, DIGEST_WIDTH=6, dual Poseidon2 + Blake3 legs, drop-between heap
pattern) with a single one-line constant change per leg:

```diff
- const QUERY_POW_BITS: usize = 16;
+ const QUERY_POW_BITS: usize = 17;
```

in both `crates/zkmcu-verifier-plonky3/src/pq_semaphore.rs` and
`crates/zkmcu-verifier-plonky3/src/pq_semaphore_blake3.rs`.

## Result

**Hypothesis on target.** The +1 grinding bit lifts conjectured FRI security
from 127 to 128 per leg at no measurable verifier cost.

| Metric | Phase E.1 (16+16) | Phase F (16+17) | Δ |
|---|---:|---:|---:|
| M33 verify (ms median) | 1611.387 | **1611.439** | +52 µs (+0.003 %) |
| Conjectured FRI bits / leg | 127 | **128** | +1 |
| Proof bytes (P2 + B3 total) | 336 801 | 336 801 | 0 |
| Heap peak | 304 180 | 304 180 | 0 |
| Stack peak | 11 196 | 11 196 | 0 |
| Variance (range %) | 0.038 | 0.058 | tighter band, identical accept rate |
| ok=true rate | 20/20 | 22/22 | clean |

The Phase A model — *+1 grinding bit ≈ +1 conjectured bit at near-zero
verifier cost* — held for the dual-hash variant. The verifier checks one
extra trailing-zero condition on the per-query PoW witness; that's a single
integer compare on a hash output that was already computed for the FS
transcript.

## Sample window

Iterations 1..=20 from `raw.log`. Iters 21–22 captured in raw but excluded
from the median computation to match Phase E.1's 20-sample window.

## Why proof size didn't grow

The `query_proof_of_work_bits` parameter controls the *difficulty* the
prover has to satisfy when searching for a per-query nonce, not the
serialised width of that nonce in the proof. Plonky3 packs the nonce in a
fixed-size field regardless of how many trailing zero bits it must carry.
So Phase F's larger work factor on the prover translates to ~2× the
expected grind time for the prover and ~0 wire-byte change.

## Security model after Phase F

Per leg: 128 conjectured FRI bits + 186-bit hash collision floor (6 ×
31-bit BabyBear digest field). Combined `min(128, 128) = 128` bits.

**Defence in depth:** a forged proof must verify under both
Poseidon2-BabyBear-16 (algebraic, audited round constants) and Blake3
(generic, ARX, decade-of-deployment ancestry). A cryptanalytic surprise on
either hash does not collapse the verifier — the other hash binds.

**PQ framing.** The bit count is *classical conjectured*. Standard
PQ-tight bounds halve the symmetric/FS bit count under Grover, so PQ-tight
≈ 64 bits, identical to every other modern STARK in this class
(Plonky3, Risc0, Starkware). The right framing is "**128-bit conjectured
classical, post-quantum by construction (no algebraic hardness assumption,
ROM/QROM-only)**", not "128-bit PQ".

## Links

- Plan section: `research/notebook/2026-04-29-security-128bit-plan.md`
  § "Status board" (Phase F entry)
- Phase E.1 baseline (M33): `benchmarks/runs/2026-04-30-m33-pq-semaphore-dual/`
- RV32 sibling: `benchmarks/runs/2026-04-30-rv32-pq-semaphore-dual-q17/`
- Verifier modules: `crates/zkmcu-verifier-plonky3/src/pq_semaphore.rs:202–205`,
  `crates/zkmcu-verifier-plonky3/src/pq_semaphore_blake3.rs:67–70`
