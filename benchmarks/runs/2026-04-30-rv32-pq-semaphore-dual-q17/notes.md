# Phase F — dual-hash at 128 conjectured bits/leg, Hazard3 RV32

RV32 sibling to `2026-04-30-m33-pq-semaphore-dual-q17/`. Identical proof
bytes, identical regen, identical iter loop — only the firmware target
changes (`riscv32imac-unknown-none-elf`). See the M33 notes for the
methodology and security-model commentary.

## Result

| Metric | Phase E.1 RV32 | Phase F RV32 | Δ |
|---|---:|---:|---:|
| Verify (ms median) | 2041.775 | **2041.085** | −690 µs (−0.034 %) |
| Conjectured FRI bits / leg | 127 | **128** | +1 |
| Heap peak | 304 180 | 304 180 | 0 |
| Stack peak | 11 000 | 11 000 | 0 |
| Variance (range %) | 0.081 | 0.054 | tighter |
| ok=true rate | 20/20 | 21/21 | clean |

The fractional speed-up vs Phase E.1 is run-to-run noise; the +1 PoW bit
is free on Hazard3 for the same reason as on M33 — a single hash compare
amortised against per-query Merkle work.

## Cross-ISA ratio

`2041.085 / 1611.439 = 1.267` — identical to Phase E.1 (1.267×). The
Hazard3 rotate-emulation tax that widens the Blake3 leg cross-ISA gap
does not amplify the PoW check itself.

## Operational caveat

Serial `cat` was started after the firmware booted, so the `[boot]` line
and iter [1] were truncated (visible only as the leading
`[boot] vec=pq-semaphore-d10-dual stack=11000 heap_base=0` fragment before
iter [2]'s output collided with it). Benchmark sample window is iters
2..=21 — 20 samples, matching Phase E.1's 20-sample window. The `stack=11000`
boot-line value matches the Phase E.1 RV32 stack peak.

## Links

- M33 sibling (full notes): `benchmarks/runs/2026-04-30-m33-pq-semaphore-dual-q17/notes.md`
- Plan section: `research/notebook/2026-04-29-security-128bit-plan.md` (Phase F entry)
- Phase E.1 RV32 baseline: `benchmarks/runs/2026-04-30-rv32-pq-semaphore-dual/`
