# Phase B — digest 4 → 6 on Hazard3 (RV32IMAC)

Companion run to `2026-04-29-m33-pq-semaphore-d6/`. Same proof.bin and
public.bin (vectors are ISA-independent), same DIGEST_WIDTH=6 verifier
build, just compiled for `riscv32imac-unknown-none-elf` and run on the
Hazard3 core of the RP2350.

## Stats

- 15 iterations clean (`ok=true`); iterations 1-5 lost in the boot-line
  serial collision (the `[fp] after parse_proof` line ran into iter 1
  and iter 6 was the first cleanly-framed verify line).
- cycles_median = 190_459_108, us_median = 1_269_727 → 1269.73 ms.
- range_pct = 0.045%, well under 0.15% target.

## Comparison

| Quantity | Phase A (RV32) | Phase B (RV32) | Δ |
|---|---|---|---|
| us_median | 1_255_981 | 1_269_727 | +13.75 ms / +1.09% |
| proof_bytes | 168_803 | 172_607 | +3_804 B |
| digest_words | 4 | 6 | +2 |
| security (FRI conj.) | 127 | 127 | unchanged |
| hash collision floor | 124 bits | 186 bits | +62 |

| Quantity | M33 | RV32 | ratio |
|---|---|---|---|
| us_median (Phase B) | 1_065_839 | 1_269_727 | 1.191× |
| us_median (Phase A) | 1_051_088 | 1_255_981 | 1.195× |

Cross-ISA ratio essentially identical → Phase B is a portability-neutral
change.

## What surprised me

Same surprise as M33: the predicted +12-22% verifier hit didn't
materialise. We measured +1.09% on RV32. Root cause analysis is in the
M33 `notes.md` — the application Merkle openings live as AIR witness
columns, not as external Merkle proofs, so digest=6 only widens the
trace by ~11 columns (~3.4%), not multiplies Poseidon2 work.

## Outstanding

- `[boot]` heap_peak / stack_peak line not captured this run — same
  serial truncation that hit the M33 capture.

## Links

- Plan section: `research/notebook/2026-04-29-security-128bit-plan.md` § Phase B
- M33 sibling run: `benchmarks/runs/2026-04-29-m33-pq-semaphore-d6/`
- Phase-A RV32 baseline: `benchmarks/runs/2026-04-29-rv32-pq-semaphore-grind32/`
