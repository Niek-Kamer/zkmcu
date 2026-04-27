# 2026-04-27 — M33 threshold-check STARK prover, N=64, BabyBear+Quartic

First run of the threshold-check circuit: prove `value=37 < threshold=100` via 32-bit
bit-decomposition. Circuit has 2 columns × 64 rows; this is the first non-Fibonacci STARK
circuit proven on a microcontroller.

## Headline

**48 ms prove, 29 ms verify, 64 KB heap peak on Cortex-M33 @ 150 MHz.**
3.1× faster prove and 3.85× less heap than Fibonacci (N=256), same 21-bit conjectured security.

## Numbers (7 iterations)

| metric | value |
|---|---:|
| trace_len | 64 |
| blowup_factor | 4 |
| num_queries | 11 |
| heap_peak | 65,880 bytes (64 KB) |
| heap_after_verify | 10 bytes |
| prove | 47,951 µs (48 ms) |
| prove spread | 47,924 – 48,052 µs (128 µs, 0.27%) |
| verify | 28,966 µs (29 ms) |
| verify spread | 28,871 – 29,047 µs (176 µs, 0.61%) |
| total | 76,923 µs (77 ms) |
| proof_bytes | 5,787 |
| security_bits | 21 (conjectured) |
| stack (boot) | 4,672 bytes |

## Comparison vs Fibonacci BB N=256 (same field, same security target)

| metric | Fibonacci BB N=256 | Threshold N=64 | ratio |
|---|---:|---:|---:|
| prove | 148.3 ms | 48.0 ms | 3.09× faster |
| verify | 32.8 ms | 29.0 ms | 1.13× faster |
| total | 181.1 ms | 76.9 ms | 2.35× faster |
| heap_peak | 253,804 bytes | 65,880 bytes | 3.85× less |
| proof_bytes | 8,970 | 5,787 | 35% smaller |

## Why verify barely improves (1.13×) despite 4× fewer rows

Both circuits use 11 queries at blowup=4. FRI query processing — opening 11 Merkle paths,
evaluating 11 out-of-domain samples, running the final polynomial check — takes roughly the
same work regardless of trace length. The trace-size-dependent term (LDE computation,
constraint evaluation) shrinks from N=1024 to N=256 LDE domain, but that term was already
not the bottleneck. The verifier is query-bound, not trace-bound. Going from N=256 to N=64
on the prover side produces the 3.1× prove speedup (LDE is 4× smaller), but barely moves
verify.

## What this proves

A sensor reading of 37 is below the safety threshold of 100. The proof is unforgeable: an
embedded device cannot generate a valid threshold proof without `value < threshold` holding
in the field. Both value and threshold are public (verifiable computation, not ZK privacy).

## Circuit design

The AIR bit-decomposes `diff = threshold - value - 1 = 62` over 32 rows. The boundary
assertion `remaining[32] = 0` certifies no field underflow, i.e., diff < 2^32, i.e.,
value < threshold. Rows 33–63 are zero-padded (transition constraint forces them).

Two constraints:
- Degree 1: `remaining[i] = 2·remaining[i+1] + bit[i]`  (shift recurrence)
- Degree 2: `bit[i]·(1 − bit[i]) = 0`  (bit is binary)

## Significance

This is the first STARK proof of a non-trivial predicate (`value < threshold`) on a bare-metal
MCU. Prior art for STARK proving on embedded systems doesn't exist (no existing work runs on
hardware with < 1 MB RAM). The threshold circuit is the hello world of verifiable IoT sensing:
an edge device attests its sensor reading without requiring trust in the device's firmware.

Next step for the circuit: make `value` private (Poseidon hash commitment) so the proof
attests "my sensor is below threshold" without revealing the exact reading.
