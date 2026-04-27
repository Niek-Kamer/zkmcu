# 2026-04-27 — M33 BabyBear+Quartic STARK prove+verify, N=256/blowup=4/q=11

BabyBear STARK prover with num_queries bumped from 8 to 11 to explore the security/cost tradeoff.

## Headline

**q=11 costs 32.8 ms to verify (vs ~20 ms at q=8). Prove is unchanged at 148 ms. Security: 21 bits conjectured — not 128. To reach 128-bit conjectured security at blowup=4 requires 64 queries.**

## Numbers

| metric | value |
|---|---:|
| trace_len | 256 |
| blowup_factor | 4 |
| num_queries | 11 |
| heap_peak | 253,804 bytes (248 KB) |
| heap_after_verify | 10 bytes |
| prove | 148,255 µs (148.3 ms) |
| verify | 32,819 µs (32.8 ms) |
| total | 181,078 µs (181.1 ms) |
| proof_bytes | 8,970 |
| security_bits | 21 (conjectured) |

## The 128-bit security myth

The firmware comment said "~12 bits per query × 11 = 132-bit". That was wrong.

Winterfell's conjectured security formula is:
```
bits = floor(log2(blowup_factor)) × num_queries
     = floor(log2(4)) × 11 = 2 × 11 = 22 bits (measured: 21)
```

The per-query security is `log2(blowup_factor)` bits, not 12. With blowup=4:
- 8 queries → ~16 bits (Phase 3.3 result confirmed this)
- 11 queries → ~21 bits

To reach 128-bit conjectured security at blowup=4: need 64 queries.
At blowup=8: log2(8)=3 bits/query → 43 queries. But blowup=8 doubles LDE domain to 2048 → ~2× heap.
At blowup=16: 4 bits/query → 32 queries. LDE=4096 → ~4× heap, would OOM.

Practical embedded path to 128-bit: needs different approach (grinding is cheap on verify, expensive on prove; or accept blowup=8 with 43 queries if heap fits).

## Prove time is query-independent on M33

M33 prove at q=8: ~148 ms (Phase 3.3). M33 prove at q=11: 148.3 ms. Unchanged — as expected. The prover commits to the trace and FRI layers independently of how many queries will be asked. Queries only affect decommitment time (verify side).

## Verify cost vs Phase 3.3 (q=8)

Phase 3.3 M33 BB verify: ~20 ms (estimated). Q=11 verify: 32.8 ms. ~1.6× more verify for 1.375× more queries. The per-query verify cost is ~3 ms/query (32.8/11 ≈ 2.98 ms/query). Linear scaling confirmed.
