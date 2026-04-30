# 2026-04-29 — Hazard3 RV32 PQ-Semaphore with 32 grinding bits (Phase A)

**What changed:** `COMMIT_POW_BITS` and `QUERY_POW_BITS` both 0 → 16 in `crates/zkmcu-verifier-plonky3/src/pq_semaphore.rs`. Same source change as the M33 pair-run; same regenerated vector.

**Why:** Phase A of `research/notebook/2026-04-29-security-128bit-plan.md`. +32 conjectured bits, near-free at the verifier.

**Headline:**
- 1255.981 ms median, 19 iterations, all ok.
- +0.51% slower than the grind=0 baseline (1249.59 ms), inside the predicted +1.0%/+12 ms band.
- 0.040% range/median variance — *tighter* than the grind=0 baseline (0.055%) and the tightest RV32 PQ-Semaphore variance to date.

**Surprise — RV32 grinding tax slightly larger than M33's.** M33 paid +0.13% for the same change; RV32 paid +0.51%. The extra Poseidon2 hash per FRI commit phase costs proportionally more on Hazard3 because it lacks UMAAL — same pattern as the phase 3.3 cross-ISA story. Cross-ISA ratio drifted from 1.190 → 1.195, basically flat.

**Bonus capture:** The boot line landed cleanly here too. `heap_peak_bytes = 298_324`, `stack_peak_bytes = 2_524`. Same heap peak as M33 (the AIR is the same, the proof is the same, only the ISA differs); RV32 stack is 320 B *smaller* than M33's 2_844 B, which tracks Hazard3's denser instruction encoding for verify-time loops.

**Success-criteria scoreboard (per plan § Phase A):**
- Cycles_median within +1.0%: ✅ +0.51%.
- Variance < 0.10%: ✅ 0.040%.
- result = ok for ≥ 16 iterations: ✅ 19/19.

**Plan link:** `research/notebook/2026-04-29-security-128bit-plan.md` § Phase A.

**Pair file:** `benchmarks/runs/2026-04-29-m33-pq-semaphore-grind32/`.
