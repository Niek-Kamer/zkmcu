# 2026-05-03 — M33 ct-reject re-run after option-C RTC verifier landed

## Headline

The 9.46x M5 timing oracle observed in
`benchmarks/runs/2026-05-03-m33-pq-semaphore-ct-reject/` is **closed**.

| Pattern | median (ms) | ratio to honest |
|---|---|---|
| honest_verify         | 1590.298 | 1.00000 |
| reject_M0_header_byte         | 1590.234 | 0.99996 |
| reject_M1_trace_commit_digest | 1590.245 | 0.99997 |
| reject_M2_mid_fri             | 1590.199 | 0.99994 |
| reject_M3_query_opening       | 1590.205 | 0.99994 |
| reject_M4_final_layer         | 1590.166 | 0.99992 |
| **reject_M5_public_byte**     | **1589.937** | **0.99977** |

All 7 patterns now sit within ±0.025% of the honest baseline. M5 went from
168 ms / 0.10565 ratio to 1589.937 ms / 0.99977 ratio.

## What changed

The CT dual-hash verifier (`pq_semaphore_dual_ct`) now dispatches both
legs through `verify_run_to_completion` instead of the upstream `verify`.
The new entry points live in vendored Plonky3:

- `vendor/Plonky3/uni-stark/src/verifier.rs::verify_run_to_completion`
- `vendor/Plonky3/uni-stark/src/verifier.rs::verify_with_preprocessed_run_to_completion`
- `vendor/Plonky3/commit/src/pcs/univariate.rs::Pcs::verify_run_to_completion`
- `vendor/Plonky3/fri/src/two_adic_pcs.rs` — `TwoAdicFriPcs::verify_run_to_completion`
- `vendor/Plonky3/fri/src/verifier.rs::verify_fri_run_to_completion`
- `vendor/Plonky3/fri/src/verifier.rs::verify_query_run_to_completion`
- `vendor/Plonky3/fri/src/verifier.rs::open_input_run_to_completion`

Every prover-controlled `?` becomes an `Option<FriError>` accumulator;
shape errors (count / dimension / range) keep their early-returns. Full
analysis: `research/postmortems/2026-05-03-rtc-verify-closing-m5.typ`.

The original `verify` / `verify_with_preprocessed` / `verify_fri` paths
are unchanged so non-CT callers stay on the fast path.

## Capture caveat

`cat /dev/ttyACM0` was attached after the firmware booted, so iters 2-5
of `honest_verify` scrolled past and iter 1's `us=` field was cut mid-line.
Honest baseline is therefore computed from 12 cycle samples and 11 µs
samples instead of 16. Range is tight enough that this doesn't affect any
conclusion (within-pattern variance ≈ 0.05%, cross-pattern variance ≈
0.025% — both well below the typical 1.0% noise floor).

## What this evidence buys

This run is the ground-truth hardware artifact for the writeup:
- The leak existed (prior run, same harness, no code change).
- The fix closes it (this run, same harness, RTC patch applied).
- The patch is a vendored Plonky3 fork (not a wrapper hack).
- The boolean correctness is preserved (host parity + ct_matches_phase_c tests).
- The host timing-parity test would now catch any future regression
  (`tests/pq_semaphore_dual_ct_timing.rs`).
