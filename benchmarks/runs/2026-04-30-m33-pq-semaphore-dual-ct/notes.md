# Phase H — M33 dual-hash CT honest path (2026-04-30)

## What this captures

26 iterations of `verify_constant_time(DUAL_PROOF_P2, DUAL_PROOF_B3, DUAL_PUBLIC)` on the unmodified `pq-semaphore-d10-dual` triple, run on the Pico 2 W (Cortex-M33 @ 150 MHz) under the firmware crate `bench-rp2350-m33-pq-semaphore-dual-ct`.

This is the **honest-path** measurement — the input parses cleanly on both legs and verify accepts. The matching reject-path measurement (the M0–M5 mutation harness through the same CT entry point) is **deferred**: the firmware crate `bench-rp2350-m33-pq-semaphore-ct-reject` is wired up, lints clean, builds clean, but on-silicon capture is currently blocked by a USB CDC-ACM enumeration / readout issue we have not isolated. See "Open issue" below.

## Scope: macro-scale CT, not instruction-level

This is macro-scale CT only. Total verify duration is what the entry point exposes; instruction-level CT (every `BabyBear` add and Poseidon2 round branch-free on secrets) is explicitly out of scope and tracked separately. See `crates/zkmcu-verifier-plonky3/src/pq_semaphore_dual_ct.rs` module-level docs.

## Result vs prediction

Phase F honest: 1611.44 ms (`benchmarks/runs/2026-04-30-m33-pq-semaphore-dual-q17/`).

CT honest measured: 1620.640 ms median (n=26).

Delta: +9.2 ms, **+0.57 %** above Phase F honest. The hardening plan budgeted +1 % for the CT path on the honest case (`research/notebook/2026-04-30-hardening-plan.md` § Phase H success criteria). Within budget.

`range_pct` = (243_164_232 − 243_003_281) / 243_096_065 = **0.066 %**, inside the < 0.1 % per-pattern target.

`heap_peak` = 304_180 B, identical to Phase F dual — confirms the drop-between pattern is preserved by `verify_constant_time`.

## Why CT is slightly slower than Phase F

Two reasons:

1. **No `?` short-circuit.** Phase C / Phase F `parse_and_verify` returns on first error, but on the honest path that early-exit point is never taken anyway, so the cost difference here is purely from the bitwise `&` composition over `is_ok()` calls, plus the public-input fallback parse attempt that runs even when the real public bytes parse cleanly.
2. **Public-input parse is double-checked.** `verify_constant_time` calls `parse_public_inputs(public)` once and stores the `Result`; on the honest path this returns Ok and the fallback branch is dead, but the stored `Result` and the boolean tracking adds a few µs.

The honest-path overhead is dominated by the second leg's verify — ~800 ms — so the constant-time bookkeeping is negligible (< 10 ms / ~0.6 %) relative to the verify cost.

## Open issue: reject-path on-silicon capture

The `bench-rp2350-m33-pq-semaphore-ct-reject` firmware iterates `mutations::ALL` (None + M0–M5 × 16 iterations) through `verify_constant_time`. On the Pi 5 / Pico 2 W loop the chip enumerates over CDC-ACM and the first `write_line` after `enumerate_for(2_000_000)` succeeds, but subsequent serial output is silently dropped — `cat /dev/ttyACM0` produces zero bytes after that first line.

We tried (none fixed it):
- HEAP=384K + Vec input (Phase C reject pattern, single-leg crate uses this and works).
- HEAP=480K + Vec input (Vec(172K) + verify peak(304K) = 476K, fits with 4K slack).
- HEAP=320K + 169K static-mut scratch buf in `.bss` (no Vec on heap).
- Replacing all formatted output with byte literals.
- Concatenating multiple stage markers into a single `write_line`.

In each case the firmware compiles and lints clean and the first `write_line` lands on the host. Subsequent ones do not. The same write pattern works in the dual-ct honest crate that produced this measurement — they share `bench-core::Bench::write_line` verbatim.

This needs targeted debugging next session — likely host-side USB enumeration sequencing on the Pi 5, or some sticky CDC-ACM device handle across reflashes. Tasks for the follow-up:

1. Capture `dmesg | tail -20` and `ls -la /dev/ttyACM*` after a fresh reject-firmware flash to see whether `/dev/ttyACM0` is being reassigned.
2. Try a longer post-flash settle (`sleep 10` instead of `sleep 3`) and a manual USB unplug-replug.
3. If still silent, instrument `bench-core::Bench::write_line` itself with a return-value checked path so we can see whether `serial.write` is failing on the second call.
4. Consider writing a `verify_constant_time_owned(p2: Vec<u8>, b3: Vec<u8>, public: &[u8])` helper that drops the input Vecs after parse, so the reject bench can use the natural `verify_constant_time` semantics without holding 172 KB of input bytes across the verify call. That removes the heap-vs-stack tightrope the current bench is walking.

The host parity tests (`crates/zkmcu-verifier-plonky3/tests/pq_semaphore_dual_ct.rs`) already cross-check the CT decision against Phase C `parse_and_verify` on canonical + every M0–M5 mutation, so the **correctness** of the CT path is verified end-to-end. What's missing is the on-silicon **timing** measurement that proves macro-CT in cycles.
