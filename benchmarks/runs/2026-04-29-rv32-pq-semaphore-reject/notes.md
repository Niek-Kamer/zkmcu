# Phase C — adversarial reject timing on Hazard3 (RV32IMAC)

Companion to `2026-04-29-m33-pq-semaphore-reject/`. Same proof bytes,
same mutation set (`zkmcu_vectors::mutations::ALL`), same firmware
loop, only the build target changes. The verifier code path itself is
literally the same Rust source compiled for `riscv32imac-unknown-none-elf`
instead of `thumbv8m.main-none-eabihf`.

## Stats

- 16 iterations per pattern. Iters 1+2 of `honest_verify` collided in
  the same serial frame (line wrap on the boot line ran into iter 1
  and iter 2 was the first cleanly framed line — same kind of
  collision the d6 RV32 capture saw on iters 1-5). 14 clean
  honest_verify iters, 16 clean iters for every reject pattern.
- Honest verify: 1302.64 ms median, 0.045 % range.
- M0 header_byte: 8.90 ms, M1 trace_commit_digest: 8.96 ms,
  M2 mid_fri: 13.94 ms, M3 query_opening: 56.03 ms,
  M4 final_layer: 56.03 ms, M5 public_byte: 151.35 ms.

## Speedups vs honest

| Pattern              | us_median | × honest |
|----------------------|----------:|---------:|
| honest_verify        | 1_302_636 |    1.00  |
| M0 header_byte       |     8_898 |  146.4   |
| M1 trace_commit_dig. |     8_964 |  145.3   |
| M2 mid_fri           |    13_944 |   93.4   |
| M3 query_opening     |    56_028 |   23.3   |
| M4 final_layer       |    56_028 |   23.3   |
| M5 public_byte       |   151_354 |    8.6   |

Worst-case attacker timing is M5 at 151.35 ms — DoS efficiency capped
at ~8.6× honest verify on Hazard3.

## Cross-ISA cost ratios

The cross-ISA ratio is not constant across patterns. Phase B verify-
only ran 1.191×; this harness shows the spread:

| Pattern              | M33 (us) | RV32 (us) | RV32/M33 |
|----------------------|---------:|----------:|---------:|
| honest_verify        | 1130_581 | 1302_636  |    1.152 |
| M0 header_byte       |    8_529 |     8_898 |    1.043 |
| M1 trace_commit_dig. |    8_579 |     8_964 |    1.045 |
| M2 mid_fri           |   13_138 |    13_944 |    1.061 |
| M3 query_opening     |   44_107 |    56_028 |    1.270 |
| M4 final_layer       |   44_090 |    56_028 |    1.271 |
| M5 public_byte       |  126_872 |   151_354 |    1.193 |

Pattern: parse-fail mutations (M0/M1/M2) are within 4–6 % across ISAs
because the work is dominated by the 172 KB heap-Vec memcpy and
postcard cursor walk — memory-bandwidth-bound. Verifier-stage
mutations (M3/M4) widen to 1.27× because Poseidon2-BabyBear-16 field
arithmetic is on the critical path, and that's where M33's UMAAL win
applies. M5 sits at 1.19×, consistent with Phase B verify-only —
it's full PCS-verify preamble cost, mostly field arithmetic.

## What surprised me

Same surprises as M33 (final-layer reject far faster than predicted,
header-byte floor set by the heap memcpy not the verifier), but with
the additional cross-ISA finding above: **the cross-ISA cost
multiplier collapses on memory-bandwidth-bound work and re-expands
on field-arithmetic-bound work**. That's a portability story to
publish — for an embedded device evaluating "do I want a STARK
verifier on M33 vs Hazard3", the answer depends on what the device
spends most of its time doing. For honest verifies it's 1.19×; for
adversarial rejects it ranges from 1.04× to 1.27× depending on which
stage rejects.

## Outstanding

- Stack peak not captured. Same caveat as M33 — boot_measure path was
  skipped for harness simplicity. Verify-only stack peak from Phase B
  is RV32 = 2 524 B.
- The RV32 honest_verify number (1302.64 ms) is +32.91 ms / +2.59 %
  above the d6 verify-only baseline (1269.73 ms). M33 paid +64.74 ms
  / +6.07 %. That asymmetry is the parse + alloc work scaling
  differently across ISAs — flagged in the result.toml verdict.

## Links

- Plan section: `bindings/.claude/plans/2026-04-29-security-128bit.md` § Phase C
- M33 sibling run: `benchmarks/runs/2026-04-29-m33-pq-semaphore-reject/`
- Honest-only Phase B baseline: `benchmarks/runs/2026-04-29-rv32-pq-semaphore-d6/`
- Mutation harness: `crates/zkmcu-vectors/src/mutations.rs`
- Firmware: `crates/bench-rp2350-rv32-pq-semaphore-reject/`
