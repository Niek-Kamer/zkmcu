# 2026-04-29 — Cortex-M33 PQ-Semaphore with 32 grinding bits (Phase A)

**What changed:** `COMMIT_POW_BITS` and `QUERY_POW_BITS` both bumped from 0 → 16 in `crates/zkmcu-verifier-plonky3/src/pq_semaphore.rs`. Vectors regenerated; firmware recompiled.

**Why:** Phase A of `bindings/.claude/plans/2026-04-29-security-128bit.md`. Grinding stacks 32 bits of conjectured security (16 commit + 16 query) on top of the 95 from `BabyBear×Quartic + 64 queries + log_blowup=1`. Total: ~127 conjectured bits. Almost free at the verifier (one extra hash + compare per FRI commit phase).

**Headline:**
- 1051.088 ms median, 23 iterations, all ok.
- +0.13% slower than the grind=0 baseline (1049.718 ms), inside the predicted +0.7%/+8 ms band.
- 0.043% range/median variance, inside the < 0.10% gate.
- 168_803 B proof — 167 bytes *smaller* than baseline.

**Surprise — proof shrunk, not grew.** Plan budgeted "+0–80 B nonce only". The grinded transcript ends up sampling a different query index set, and those queries' Merkle openings encode into fewer postcard varint bytes. Net delta dominates the 16-byte nonce additions. No soundness concern — the proof still verifies — but a useful reminder that proof size at fixed query-count is content-dependent under varint encoding.

**Bonus capture:** The boot_measure `[boot]` line landed cleanly this run (it had been dropped on the grind=0 capture). So we now have `heap_peak_bytes = 298_324` and `stack_peak_bytes = 2_844` for this AIR — the `heap_peak_estimate ≈ 280_000` from the baseline notes was within 7% of truth.

**Success-criteria scoreboard (per plan § Phase A):**
- Cycles_median within +0.7%: ✅ +0.13%.
- Variance < 0.10%: ✅ 0.043%.
- result = ok for ≥ 16 iterations: ✅ 23/23.

**Plan link:** `bindings/.claude/plans/2026-04-29-security-128bit.md` § Phase A.

**Pair file:** `benchmarks/runs/2026-04-29-rv32-pq-semaphore-grind32/` (same change, RV32 silicon).
