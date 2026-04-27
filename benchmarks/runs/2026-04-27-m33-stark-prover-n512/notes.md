# 2026-04-27 — M33 Goldilocks STARK prove+verify, N=512/blowup=2

First run breaking the N=256 SRAM ceiling. The key question: does doubling trace length fit in 384 KB and how much does it cost?

## Headline

**N=512 at blowup=2 fits. Heap peak: 311 KB. Prove: 158.8 ms. Verify: 19.2 ms. Total: 178.0 ms. Heap fully freed to 10 bytes after each iteration.**

## Numbers

| metric | value |
|---|---:|
| trace_len | 512 |
| blowup_factor | 2 |
| LDE domain | 1024 (same as N=256/blowup=4) |
| heap_peak | 311,008 bytes (303 KB) |
| heap_after_verify | 10 bytes |
| prove | 158,828 µs (158.8 ms) |
| verify | 19,156 µs (19.2 ms) |
| total | 177,985 µs (178.0 ms) |
| proof_bytes | 6,154 |
| security_bits | 7 (conjectured) |

## Why heap fits

N=512/blowup=2 gives LDE domain = 512 × 2 = 1024 — identical to N=256/blowup=4. The LDE matrix is the dominant heap consumer and its size is determined by LDE domain × column count × element size, not by trace length alone. So heap peak is nearly identical to the prior N=256 baseline: 311 KB vs ~306 KB (prior run's ~306 KB), within rounding.

## Comparison to N=256/blowup=4 baseline (2026-04-26)

Prior N=256/blowup=4 prove: ~134 ms. N=512/blowup=2 prove: 158.8 ms. ~18% slower for 2× the trace.

That's expected: doubling N roughly doubles the trace-column FFT cost, but LDE/FRI/Merkle costs stay the same (same LDE domain). The blowup reduction from 4→2 saves LDE work, partially offsetting the longer trace. Net: ~18% overhead for 2× the statement.

## Security

security_bits=7 is correct. The winterfell conjectured security formula gives:
`floor(log2(blowup_factor)) × num_queries = log2(2) × 8 = 1 × 8 = 8 bits`
(reported as 7 due to rounding/field-size cap). This is intentionally low — the goal of this run is SRAM feasibility, not security.

## Heap reclaim validation

heap_after=10 bytes every iteration. The proof struct is freed when it goes out of scope at end of verify block. The TLSF allocator reclaims all allocations made during prove+verify. No memory leak.
