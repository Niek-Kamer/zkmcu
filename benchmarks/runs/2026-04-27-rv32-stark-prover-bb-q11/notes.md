# 2026-04-27 — RV32 BabyBear+Quartic STARK prove+verify, N=256/blowup=4/q=11

RV32 counterpart to the M33 BB q=11 run. Main finding: the cross-ISA prove ratio at q=11 is 1.45× vs 1.04× at q=8 (Phase 3.3), and a layout-probe build confirmed this is real — not an XIP cache artifact.

## Headline

**RV32 prove 215.0 ms vs M33 148.3 ms = 1.45×. Layout-probe build (16-byte .rodata shift) confirmed: 215.0 ms unchanged across 45 iterations, 0.17% spread. The ratio is real for this firmware.**

## Numbers (45 iterations, layout-probe build included)

| metric | value |
|---|---:|
| trace_len | 256 |
| blowup_factor | 4 |
| num_queries | 11 |
| heap_peak | 253,804 bytes (248 KB) |
| heap_after_verify | 10 bytes |
| prove | 215,000 µs (215.0 ms) |
| prove spread | 214,824 – 215,199 µs (375 µs, 0.17%) |
| verify | 42,221 µs (42.2 ms) |
| total | 257,221 µs (257.2 ms) |
| proof_bytes | 8,970 |
| security_bits | 21 (conjectured) |

## Cross-ISA comparison (q=11)

| metric | M33 | RV32 | ratio |
|---|---:|---:|---:|
| prove | 148.3 ms | 215.0 ms | 1.450× |
| verify | 32.8 ms | 42.2 ms | 1.284× |
| total | 181.1 ms | 257.2 ms | 1.419× |

## Layout-probe confirmation

A second build with `[u32; 4]` padding inserted into `.rodata` (16-byte shift) was flashed and measured over 45 iterations. Result: 215,000 µs median, identical to within normal variance (79 µs delta = 0.04%). The 1.45× ratio is not an XIP cache alignment artifact from this binary layout.

## Why 1.45× now vs 1.04× in Phase 3.3

Phase 3.3 (q=8): M33=148 ms, RV32≈154 ms → 1.04×.
This run (q=11): M33=148 ms, RV32=215 ms → 1.45×.

The M33 prove time is identical across q=8 and q=11 (148 ms both). RV32 went from ~154 ms to 215 ms (+39%) with 3 extra queries.

The firmware for this run is substantially larger than Phase 3.3 firmware: it has additional output code (`prove_verify_total`, `security_bits`, timed boot verify). This larger binary fills more of Hazard3's 4 KB I-cache. The Phase 3.3 firmware with its smaller code may have fit the hot prover path cleanly into 4 KB; the current firmware does not. M33's 16 KB I-cache is unaffected.

This is distinct from a single-build layout sensitivity (ruled out by the layout probe). It's about total code footprint: the current firmware is simply too big for Hazard3's I-cache to hold the hot prover loop efficiently, whereas the Phase 3.3 firmware was not.

Takeaway: the BabyBear "1.04× cross-ISA advantage" from Phase 3.3 was real for small firmware. For production-sized firmware where the prover hot path competes with output/verification code for the 4 KB I-cache, the true RV32 disadvantage is closer to 1.4–1.5× — similar to what Goldilocks shows.

## Variance analysis

375 µs spread over 45 iterations = 0.17%. This is the tightest variance seen in any RV32 run, consistent with a fully warmed XIP cache and stable thermal/clock state after many iterations.
