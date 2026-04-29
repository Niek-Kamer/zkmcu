# Phase B — digest 4 → 6 on Cortex-M33

Phase B of the 128-bit security plan
(`.claude/plans/2026-04-29-security-128bit.md`): widen the application
Semaphore Merkle digest from 4 BabyBear elements (124-bit digest space)
to 6 (186-bit). Goal: remove the hash-collision floor as the soundness
bottleneck. FRI-side stays at 127 conjectured bits from Phase A.

## What changed

- `DIGEST_WIDTH` 4 → 6 in `crates/zkmcu-verifier-plonky3/src/pq_semaphore.rs`.
- `CAPACITY_START` derived as `2 * DIGEST_WIDTH` (was hardcoded `8`).
- `NUM_PUBLIC_INPUTS` derived as `4 * DIGEST_WIDTH` (was hardcoded `16`),
  so public.bin grew from 64 B to 96 B.
- All `[T; DIGEST_WIDTH]` columns and the AIR's public-input offsets
  rescale automatically.
- Host-gen seeds extended from 32 to 48 bytes (8 bytes per digest
  element).

## Capacity audit risk: not a real risk

The plan flagged a possible audit gap because the `hash_pair` input
fills 12 of 16 state slots, leaving 4-element capacity (vs the previous
8). Resolution: this is **not** a sponge with absorption — every active
row hashes a single fixed-size input via one Poseidon2 permutation,
truncated to the digest width on output. Audited round constants are
valid for any input distribution; capacity in this construction is
zero-padding, not sponge capacity in the absorption-soundness sense.
No change to width-16; no migration to Poseidon2-24.

## What surprised me

**Verify time grew far less than predicted.** Plan said +12-22%;
measured +1.40% (1051.088 → 1065.839 ms median).

The plan's reasoning ('more Poseidon2 inputs to absorb') was off:
- The verifier always evaluates the full width-16 Poseidon2 permutation
  on every active row regardless of how many input slots are populated.
  Going from 4 to 6 populated input slots does not add Poseidon2 work.
- The actual cost is the trace-width delta. Each of the four
  digest-sized witness columns (`prev_digest`, `sibling`, `id_col`,
  `scope_col`) gained 2 elements, ≈ 8 extra columns. Plus a few
  bytes of `repr(C)` alignment padding. Total trace-width bump
  ≈ 11 columns, ~3.4% wider trace, which propagates into the LDE
  size and the FRI commits.

**Proof size grew far less than predicted.** Plan said +50% (215-235 KB);
measured +2.3% (172_607 B). Same root cause: the application Merkle
openings are not extra proof bytes — they are trace columns committed
inside the FRI MMCS. The proof contains FRI commits + query openings
of those columns, not external Merkle paths.

## Stats

- 20 iterations, all `ok=true`.
- cycles_median = 159_875_849, us_median = 1_065_839.
- range_pct = 0.032% (vs 0.043% on Phase A; comparable variance).

## Outstanding

- `[boot]` heap_peak / stack_peak line not captured — serial cut at
  iter 21 before the firmware reached its summary. heap_after_parse
  (156_360 B) is recorded; full-run peak isn't. Re-flash with longer
  capture if the whitepaper needs the full footprint number.

## Links

- Plan section: `bindings/.claude/plans/2026-04-29-security-128bit.md` § Phase B
- Phase-A baseline: `benchmarks/runs/2026-04-29-m33-pq-semaphore-grind32/`
- Vectors regenerated: `crates/zkmcu-vectors/data/pq-semaphore-d10/`
  (proof.bin 172_607 B, public.bin 96 B)
