# Phase D — Goldilocks × Quadratic verify on Cortex-M33

Phase D of the 128-bit security plan exists to test the hypothesis that
Goldilocks × Quadratic — 128-bit native field, no conjecture-stack on
the field side — would also be **faster** than the BabyBear × Quartic
Phase A+B baseline on M33. The Phase 3.3 STARK comparisons measured
Goldilocks × Quadratic at ~66% of the BabyBear × Quartic cost on M33
for fib1024, so the plan's predicted band was 600–680 ms.

## Result

**Hypothesis rejected**.

| Metric | Phase B (BB-d6) | Phase D (GL-Quad) | Δ |
|---|---:|---:|---:|
| M33 verify (ms) | 1065.84 | 1995.66 | **+87 %** |
| Proof size (bytes) | 172 607 | 271 120 | +57 % |
| Heap peak (bytes) | 329 079 | 415 312 | +26 % |
| Variance (range %) | 0.149 | 0.028 | tighter (still excellent) |

Phase D's verify time is +87% over the BabyBear-d6 baseline on M33, far
outside the predicted 600–680 ms band. The plan's prediction inherited
Phase 3.3's "Goldilocks 66% faster" number from a completely different
shape — fib1024 is arithmetic-bound (state additions, S-boxes on a
small trace) while PQ-Semaphore verify is hash-bound (64 FRI queries ×
~10 Merkle hops × Poseidon2 permutations dominate the cycle budget).

## Why hash-heavy verify favours BabyBear, not Goldilocks

Two compounding effects:

1. **64-bit elements on a 32-bit MCU**. Every Goldilocks mul, add, sub
   is multi-precision. The M33's UMAAL instruction (32×32+32+32→64
   multiply-accumulate) is exactly the primitive needed for Goldilocks
   modular reduction, but even with UMAAL each base-field op is
   roughly 3–4× a BabyBear (31-bit) op.

2. **More Poseidon2 rounds**. `GOLDILOCKS_POSEIDON2_PARTIAL_ROUNDS_16`
   is **22**; `BABYBEAR_POSEIDON2_PARTIAL_ROUNDS_16` is **13**. That's
   1.7× more partial rounds per permutation, on top of element-wise
   ops being more expensive. The S-box is x^7 in both; with 64-bit
   arithmetic the modular cube and squaring are materially heavier.

Multiplying: ~3.5× per-op slowdown × 1.7× partial rounds ≈ 6× per-
permutation hash cost. Then divide by the AIR-side savings from
DIGEST_WIDTH dropping 6→4 (~33% fewer absorptions per row) and the
internal MMCS DIGEST_ELEMS dropping 8→4 (~50% fewer Merkle path
hashes), and you land at the observed +87%.

## Heap budget

The 480 KB heap (bumped from Phase B's 384 KB) was needed because the
parsed `Proof<GoldilocksConfig>` lands at ~245 KB right after parse
(~1.9× the 271 KB wire size, same expansion ratio as Phase B), and
verify scratch peaks an additional ~170 KB on top. Total peak 415 KB
fits comfortably in 480 KB.

The 480 KB heap left only ~32 KB for stack growth. `boot_measure` was
removed from the GL firmware because `bench_core::measure_stack_peak`
paints a 64 KB sentinel below SP — that paint runs off the bottom of
the stack region and corrupts the heap allocator state, panicking on
the first verify allocation. Same harness simplification the Phase C
reject benches accepted. Stack peak is not captured for this run.

## Proof size

271 KB on the wire vs Phase B's 172 KB (+57 %), despite halving both
the AIR digest (6→4 elements) and the internal MMCS digest (8→4
elements). Two things drove the growth:

- Each Goldilocks element is 8 bytes vs 4 for BabyBear — every Merkle
  path bytes count doubles per node hash.
- Quadratic vs Quartic extension: each commit-phase opening is
  smaller (2 elements vs 4), but per-query openings carry 64-bit base-
  field elements throughout, partially undoing the win.

The plan's prediction of 130–160 KB was based on "smaller field
elements" — that prediction missed the 1.6× per-element byte penalty
that Goldilocks pays vs BabyBear.

## Takeaway

For an embedded PQ-Semaphore verifier, BabyBear × Quartic + Phase A+B
grinding stays the right choice. Goldilocks × Quadratic delivers
128-bit *native* field security but at +87% verify cost on M33. The
two configs together populate the 2x2 table the plan's "Decision
point after Phase D" anticipated:

| Config | M33 (ms) | Conj | Native field | Cross-ISA | Proof |
|---|---:|---:|---:|---:|---:|
| BB × Quartic + d6      | 1066 | 127 | 124-bit (stacked) | 1.19× | 172 KB |
| GL × Quadratic + grind | 1996 | 127 | 128-bit (native) | 1.35× | 271 KB |

Reader picks. Don't kill the BabyBear path.

## Links

- Plan section: `bindings/.claude/plans/2026-04-29-security-128bit.md` § Phase D
- RV32 sibling: `benchmarks/runs/2026-04-29-rv32-pq-semaphore-gl/`
- Phase B BB-d6 baseline (M33): `benchmarks/runs/2026-04-29-m33-pq-semaphore-d6/`
- Verifier module: `crates/zkmcu-verifier-plonky3/src/pq_semaphore_goldilocks.rs`
- Host-gen: `crates/zkmcu-host-gen/src/pq_semaphore_gl.rs`
- Firmware: `crates/bench-rp2350-m33-pq-semaphore-gl/`
