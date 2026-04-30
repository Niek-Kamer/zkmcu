# Phase E.1 — stacked dual-hash verify on Hazard3 (RV32IMAC)

Companion to `2026-04-30-m33-pq-semaphore-dual/`. Same proof bytes,
same loop, only the firmware target changes.

## Stats

- 20 iterations.
- Honest dual verify: 2041.78 ms median, 0.081 % range.
- Heap peak: 304 180 B (same as M33, allocator state is target-
  independent).
- Stack peak: 11 000 B — 64 KB sentinel paint succeeded with 384 KB heap.
- All 20 iters returned `ok=true`.

## Cross-ISA cost

| Phase | M33 (ms) | RV32 (ms) | RV32 / M33 |
|---|---:|---:|---:|
| A (BB grind only)            | 1051.09 | 1255.98 | 1.195 |
| B (BB d6 + grind)            | 1065.84 | 1269.73 | 1.191 |
| C (BB full pipeline)         | 1130.58 | 1302.64 | 1.152 |
| D (GL × Quadratic)           | 1995.66 | 2700.84 | 1.354 |
| **E.1 (dual P2 + Blake3)**   | **1611.39** | **2041.78** | **1.267** |

Phase E.1 widens the cross-ISA gap from Phase B's 1.19× to 1.27×. The
Phase B Poseidon2 leg keeps its 1.19× tax (BabyBear field arithmetic
is similar across the two ISAs, with UMAAL helping M33 and Hazard3
single-cycle ALU paths matching). The widening comes from the **Blake3
leg**:

  - Per-leg estimate (dual minus Phase B):
    - p2 leg: M33 ~1066 ms / RV32 ~1270 ms → ratio ~1.19×
    - b3 leg: M33  ~545 ms / RV32  ~772 ms → ratio ~1.42×

Hazard3 lacks a single-instruction barrel rotate; it emulates
`x.rotate_right(n)` as `srl rd, rs, n ; sll tmp, rs, 32-n ; or rd, rd, tmp`,
paying 3 instructions per rotate where Cortex-M33 pays 1. Blake3's ARX
inner loop has 8 rotates per round × 7 rounds × ~64 queries × ~10 Merkle
hops × ~7 Blake3 calls per node — that compounds to a measurable extra
22 % verify cost on Hazard3.

## Hypothesis verdict

Plan predicted 2700-3000 ms RV32; measured **2041.78 ms — +60.8 %** over
Phase B baseline, well below the predicted band. Same root cause as the
M33 sibling (Blake3 is materially cheaper than Poseidon2 on this CPU
shape), partially offset by Hazard3's missing rotate.

## Per-leg portability story

For the paper:
  - **BabyBear-d6 only (Phase B)** — 1.19× cross-ISA, single-hash,
    127 conjectured bits. Right pick for callers who accept algebraic-
    hash conjecture stack and want the tightest portability story.
  - **Dual (Phase E.1)** — 1.27× cross-ISA, dual-hash, hash-tower
    soundness composition. Right pick for callers who want
    cryptanalytic backup. Pays a ~50-60 % verify-time premium over
    the single-hash row.

Reader picks. The Phase B path is not killed.

## Outstanding

None. All success criteria from the plan met:
- Both sub-verifies accept ✓
- Variance < 0.10 % ✓
- Combined verify under 2.5 s on M33 (1.6 s, well clear) ✓
- Heap budget within 384 KB (304 KB peak) ✓
- Stack peak captured (11 KB) ✓

## Links

- Plan section: `bindings/.claude/plans/2026-04-29-security-128bit.md` § Phase E
- M33 sibling: `benchmarks/runs/2026-04-30-m33-pq-semaphore-dual/`
- Phase B P2 baseline (RV32): `benchmarks/runs/2026-04-29-rv32-pq-semaphore-d6/`
- Verifier modules: `crates/zkmcu-verifier-plonky3/src/pq_semaphore_blake3.rs`, `crates/zkmcu-verifier-plonky3/src/pq_semaphore_dual.rs`
- Firmware: `crates/bench-rp2350-rv32-pq-semaphore-dual/`
