# Phase E.1 — stacked dual-hash (Poseidon2 + Blake3) verify on Cortex-M33

Phase E.1 of the 128-bit security plan. Same Phase B AIR (BabyBear ×
Quartic, DIGEST_WIDTH=6, 64 FRI queries, 16+16 grinding bits), proven
twice — once under audited Poseidon2-`BabyBear`-16 and once under Blake3
— and verified twice. The dual verifier rejects iff either leg rejects.

## Result

**Hypothesis exceeded** (in the good direction).

| Metric | Phase B (P2 only) | Phase E.1 (P2 + B3) | Δ |
|---|---:|---:|---:|
| M33 verify (ms) | 1065.84 | 1611.39 | **+51.2 %** |
| Combined proof bytes | 172 607 | 336 801 | +95.1 % |
| Heap peak (bytes) | 329 079 | 304 180 | -7.6 % |
| Variance (range %) | 0.149 | 0.038 | tighter |
| Stack peak (bytes) | — | 11 196 | sentinel paint succeeded |

Phase E.1's verify time is +51% over the BabyBear-d6 Phase B baseline,
**well below the plan's predicted 2200-2500 ms band** (which assumed dual
≈ 2 × Phase B). The Blake3 leg is materially cheaper than the Poseidon2
leg on Cortex-M33 — the hash-bound ratio P2:B3 is roughly 2:1.

## Why Blake3 is faster than Poseidon2 here

Per-leg cost estimate (subtracting Phase B baseline from dual; ignores
parse-step accounting which is identical between legs):
  - **P2 leg ≈ 1066 ms.** Width-16 Poseidon2-`BabyBear` permutation
    on every Merkle node and FRI absorption. 21 rounds (8 full, 13
    partial) of S-box (x^7), MDS, round constant. Each round is a
    handful of `BabyBear` multiplications + adds — Cortex-M33's UMAAL
    helps, but field arithmetic is the bottleneck.
  - **B3 leg ≈ 545 ms.** Blake3's compression function is an ARX inner
    loop: 32-bit add, rotate, xor. Cortex-M33 ships a 1-cycle barrel
    shifter / `ror` instruction; the ARX schedule unrolls cleanly into
    SIMD-free straight-line code that LLVM optimises aggressively.
    No field arithmetic at all on this leg.

So even though Blake3 hashes more raw bytes per Merkle path than
Poseidon2 does (32-byte vs 24-byte digest), the per-byte cost is so
much lower that the leg lands at roughly half the cost.

## Heap budget

The dual entry point at `pq_semaphore_dual::parse_and_verify` parses the
Poseidon2 leg in a scoped block, drops it (Rust destructors free the
heap), then parses the Blake3 leg. Peak heap is `max(p2_peak, b3_peak)`
not the sum. Measured: 304 KB peak, comfortably inside a 384 KB heap
budget. Stack peak 11 KB — sentinel paint succeeded with the regular
Phase B budget, no need for the Phase D 480 KB heap workaround.

## Proof size

Total 336.8 KB on the wire (172.9 KB Poseidon2 + 163.8 KB Blake3),
landing at the low edge of the predicted 350-420 KB band. Blake3's
32-byte digests pack tighter than 6 × 4-byte BabyBear digests inside
the FRI MMCS Merkle paths, so the b3 leg is slightly smaller than the
p2 leg despite hashing more raw bytes per path entry.

## Security composition

Both legs verify the same statement (same Merkle root, same nullifier,
same scope_hash) under cryptographically independent hash functions:
  - **Poseidon2-BabyBear-16**: algebraic, audited round constants,
    well-suited to inside-the-circuit witness generation but a younger
    target for cryptanalysis.
  - **Blake3**: generic, ARX-design, decade-of-deployment ancestry
    (BLAKE → BLAKE2 → BLAKE3), no algebraic structure to exploit.

A forged proof must verify under BOTH. Even if one hash suffers a
cryptanalytic surprise (Poseidon2 is the more conservative target here),
the verifier still requires the other to accept. Combined soundness is
a function of `min(127, 127) = 127` conjectured FRI bits per leg, plus
the hash-tower property that an attacker has to fool both functions
simultaneously.

## Takeaway

For the "production" 128-bit-class headline this is the row to put in
the paper:
  - 1.6 s verify on Cortex-M33
  - 384 KB heap (no special workarounds)
  - 337 KB total proof bytes
  - 127 conjectured FRI bits per leg + dual-hash composition
  - 1.27× cross-ISA portability (M33 ↔ Hazard3, see RV32 sibling)

## Links

- Plan section: `bindings/.claude/plans/2026-04-29-security-128bit.md` § Phase E
- RV32 sibling: `benchmarks/runs/2026-04-30-rv32-pq-semaphore-dual/`
- Phase B P2 baseline (M33): `benchmarks/runs/2026-04-29-m33-pq-semaphore-d6/`
- Verifier modules: `crates/zkmcu-verifier-plonky3/src/pq_semaphore_blake3.rs`, `crates/zkmcu-verifier-plonky3/src/pq_semaphore_dual.rs`
- Host-gen: `crates/zkmcu-host-gen/src/pq_semaphore_dual.rs`
- Firmware: `crates/bench-rp2350-m33-pq-semaphore-dual/`
