# 2026-04-27 — RV32 BabyBear+Quartic STARK prove+verify, N=256/blowup=4/q=11

RV32 counterpart to the M33 BB q=11 run. Most interesting result: the cross-ISA ratio for prove widened dramatically compared to the Phase 3.3 q=8 run.

## Headline

**RV32 prove: 214.9 ms vs M33 148.3 ms = 1.45× cross-ISA gap. At q=8 (Phase 3.3) the gap was 1.04×. The 3 extra queries hit RV32 disproportionately hard — likely XIP cache layout sensitivity.**

## Numbers

| metric | value |
|---|---:|
| trace_len | 256 |
| blowup_factor | 4 |
| num_queries | 11 |
| heap_peak | 253,804 bytes (248 KB) |
| heap_after_verify | 10 bytes |
| prove | 214,921 µs (214.9 ms) |
| verify | 42,154 µs (42.2 ms) |
| total | 257,073 µs (257.1 ms) |
| proof_bytes | 8,970 |
| security_bits | 21 (conjectured) |

## Cross-ISA comparison (q=11)

| metric | M33 | RV32 | ratio |
|---|---:|---:|---:|
| prove | 148.3 ms | 214.9 ms | 1.450× |
| verify | 32.8 ms | 42.2 ms | 1.284× |
| total | 181.1 ms | 257.1 ms | 1.419× |

## Cross-ISA prove ratio anomaly

Phase 3.3 (q=8): M33=148 ms, RV32≈154 ms → 1.04×.
This run (q=11): M33=148.3 ms, RV32=214.9 ms → 1.45×.

M33 prove time is identical across both runs (148 ms). RV32 went from ~154 ms to 214.9 ms (+39%) by changing only num_queries from 8 to 11. This is too large to be explained by the 3 extra Merkle decommitments alone.

Most likely explanation: XIP flash cache sensitivity on Hazard3. Adding 3 queries changes the hot FRI query-response loop structure, shifting binary code layout and invalidating favorable cache line alignment that existed in the q=8 build. M33 has a larger instruction cache and is less sensitive to this effect.

This means the Phase 3.3 1.04× ratio may have been partially lucky cache alignment on RV32. The "true" BabyBear cross-ISA ratio at blowup=4 is probably somewhere between 1.04× and 1.45×, and varies with build.

## Verify cost comparison

M33 verify at q=11: 32.8 ms. RV32 verify at q=11: 42.2 ms → 1.284× ratio.
The verify ratio (1.28×) is more consistent with prior expectations for Merkle-heavy code (mixed Blake3 + field ops, less dominated by the 64-bit Goldilocks UMAAL advantage).
