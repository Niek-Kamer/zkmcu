# 2026-04-27 — RV32 Goldilocks STARK prove+verify, N=512/blowup=2

RV32 counterpart to the M33 N=512 run. Same firmware parameters, different CPU core.

## Headline

**N=512 at blowup=2 fits on RV32 too. Heap peak: 311 KB. Prove: 213.8 ms. Verify: 23.4 ms. Total: 237.2 ms. Heap fully freed each iteration.**

## Numbers

| metric | value |
|---|---:|
| trace_len | 512 |
| blowup_factor | 2 |
| LDE domain | 1024 |
| heap_peak | 311,008 bytes (303 KB) |
| heap_after_verify | 10 bytes |
| prove | 213,824 µs (213.8 ms) |
| verify | 23,381 µs (23.4 ms) |
| total | 237,183 µs (237.2 ms) |
| proof_bytes | 6,154 |
| security_bits | 7 (conjectured) |

## Cross-ISA comparison for Goldilocks N=512

| metric | M33 | RV32 | ratio |
|---|---:|---:|---:|
| prove | 158.8 ms | 213.8 ms | 1.346× |
| verify | 19.2 ms | 23.4 ms | 1.220× |
| total | 178.0 ms | 237.2 ms | 1.332× |

The 1.35× prove ratio is consistent with previous Goldilocks observations (~1.35–1.55×, build-dependent due to XIP cache layout). Goldilocks 64-bit field mul benefits from M33's UMAAL instruction; Hazard3 does it as two MUL/MULHU pairs.

## Heap identical across ISAs

heap_peak=311,008 on both M33 and RV32. heap_after=10 on both. Proof bytes=6,154 on both. All proof parameters are ISA-independent — they depend only on field, extension, trace, and FRI parameters.
