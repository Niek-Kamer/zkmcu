# 2026-05-03 — M33 PQ-Semaphore CT reject characterisation

## Summary

First end-to-end run of `bench-rp2350-m33-pq-semaphore-ct-reject` on
hardware. 7 patterns × 16 iterations = 112 verifies, all completed
cleanly (`[done] all patterns complete` printed at end of log).

**Two findings worth flagging:**

1. **The bench works at all only because of a two-stage CT API added
   this session.** The original one-stage `verify_constant_time(&[u8],
   &[u8], &[u8])` keeps the raw 169 KB proof_p2 bytes alive throughout
   the call, which combined with the verifier's ~304 KB peak doesn't
   fit in the 384 KB heap → silent OOM crash via `panic_halt`. The new
   `parse_*_constant_time` + `verify_*_leg_constant_time` split lets
   the bench drop the raw bytes after parse, before the timed window
   starts. See `findings/2026-05-03-ct-reject-debugging.md` for the
   bisection that established this.

2. **`verify_constant_time` is NOT macro-CT for `M5_public_byte`.**
   M0–M4 reject within 0.05 % of honest (1.5936 s vs 1.5936 s honest
   median). M5 rejects in 168 ms — **9.46x faster than honest**, a
   clear timing oracle. The host-side `ct_matches_phase_c_on_every_mutation`
   test only checks boolean parity, never timing — so this slipped
   through. See `findings/2026-05-03-ct-verifier-m5-public-input-leak.md`
   for the leak's mechanism and remediation options.

## Capture conditions

- Built via `just build-m33-pq-semaphore-ct-reject` on the dev machine.
- scp'd ELF to Pi 5 at `pid-admin@10.42.0.30:/tmp/bench-m33-ct-reject.elf`.
- User manually held BOOTSEL on the Pico 2 W, then ran `picotool load
  -v -x -t elf` on the Pi.
- Capture: `timeout 240 cat /dev/serial/by-id/usb-zkmcu_bench-rp2350-m33-pq-semaphore-ct-reject_0001-if00 | tee raw.log`.
  240 s was enough for all 112 iterations + the `[done]` line; the
  timeout fired *after* the bench finished.

## What the heap numbers say

`heap_base=156360` is constant across all iterations and patterns. That's
the parsed-p2 `Proof` (the largest contributor) plus the parsed public
inputs, alive at the moment `measure_cycles` starts.

`heap_peak=304180` is also constant across all iterations and patterns,
including M5. So the heap allocator path runs identically across
mutations — confirming that M5's 9.46x speedup happens *inside* the
verifier proper (the FRI commit phase, given when the early-out fires),
not in any pre-verify allocation drift.

The 304 KB peak matches the Phase B measurement of one-stage
`verify_constant_time` with static refs, validating the "two-stage with
explicit drops keeps the same peak as one-stage with statics" design
assumption.

## What this run does NOT cover

- Stack peak (`stack_peak_bytes = 0`). `bench-core::measure_stack_peak`
  paints a 64 KB sentinel below SP and we deliberately don't call it
  here — the original ct-reject bisection wrongly suspected it of the
  hang, and although that hypothesis was falsified, exercising the
  paint path with this heap layout still has not been verified safe.
  Stack characterisation is deferred.

- Hazard3 (rv32) sibling. The `bench-rp2350-rv32-pq-semaphore-ct-reject`
  crate exists but wasn't run this session. Whatever heap-budget fix
  shipped here for M33 needs to be ported / verified for rv32 before
  the rv32 sibling can produce comparable numbers.

- The CT property at the API level. `verify_constant_time` (one-stage)
  was not exercised in this session beyond the unchanged host tests.
  The new two-stage entry points are what the bench measured. The two
  paths share their entire verifier internals via the refactor, so
  M5's leak is presumed identical, but that was not directly measured.
