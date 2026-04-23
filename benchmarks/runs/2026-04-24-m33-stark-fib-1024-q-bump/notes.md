# 2026-04-24 — Cortex-M33 STARK verify, Quadratic + BumpAlloc

Phase 3.2.y follow-up to the variance-isolation report. The phase 3.2.x
clone-hoist experiment disconfirmed the `proof.clone()` hypothesis and
pointed at winterfell's ~400 internal `Vec` allocations as the true
variance source. This run tests the real fix: replace `LlffHeap`
entirely with a bump allocator that resets to a fixed watermark
between iterations, so each verify call starts with identical
allocator state.

## Headline

**67.95 ms median**, **0.082 % IQR variance**, 27 clean iterations.
All `ok=true`. Variance is now at silicon baseline.

## What changed

Two things, in a single commit:

- New crate `zkmcu-bump-alloc` with a `BumpAlloc` type implementing
  `GlobalAlloc`. Key features: atomic-CAS bump pointer, `dealloc` is a
  no-op, `realloc` is in-place when the resized allocation is on top
  of the bump (covers the common `Vec::push` case), watermark
  save/restore.
- Firmware switched global allocator from `TrackingHeap(LlffHeap)` to
  `BumpAlloc`. Arena bumped to 384 KB because bump-with-no-op-dealloc
  accumulates "leaked" memory from every non-top `realloc`. Each loop
  iteration resets to a watermark captured right after `parse_proof`.

## Variance result

Three variance measures, reporting each for transparency:

| Measure | Value | Comparison |
|---|---:|---|
| min-max / median | 0.352 % | noisier than LlffHeap's 0.245 % |
| *IQR / median* | **0.082 %** | **36 % tighter than LlffHeap** |
| std-dev / mean | 0.080 % | at silicon baseline |

The min-max spread is worse than LlffHeap because there are 3 outlier
iterations (iter 8: 68.108 ms, iter 11: 68.077 ms, iter 18: 68.047 ms)
that extend the max. But 24 of 27 iterations cluster within
67.869–67.996 ms — a 127 μs band, 0.187 % of median.

IQR and std-dev both land at ~0.08 %, which is where BN254 Groth16
and BLS12-381 Groth16 verifiers measured on the same silicon (no
allocator activity). *The allocator was the variance source, and
bump alloc removed it.* The remaining min-max outliers look like
cache eviction or USB-peripheral interference — effects below the
allocator layer that a different fix would need to address.

## Performance side effect

Median verify time dropped from 69.67 ms (LlffHeap clone-hoisted) to
67.95 ms — *1.72 ms faster*. The `LlffHeap` free-list traversal is
pure overhead on the happy path; `BumpAlloc`'s O(1) bump-a-pointer
eliminates it. This quantifies the allocator-cost component of the
overall verify time: ~1.7 ms or ~2.5 % of verify work was the
linked-list first-fit allocator.

## Memory cost

`heap_peak` = 314,504 B. Up from LlffHeap's 93,515 B — that's 221 KB
of "waste" from non-top reallocs that the in-place realloc
optimisation couldn't handle. The fallback `alloc + copy + no-op
dealloc` path leaks the old slot.

This takes us *above* the 128 KB hardware-wallet tier. BumpAlloc
is therefore a *benchmark / variance-measurement tool*, not a
production configuration. Production firmware should use `LlffHeap`
or `TlsfHeap` and accept the ~0.25 % variance, OR refactor winterfell
to use pre-allocated scratch arenas (Phase 4 engineering).

The finding that production STARK verify fits under 128 KB on
`LlffHeap` (93.5 KB peak, 31 KB margin) still stands — that is the
deployment number. The bump-alloc number is *what variance looks
like when the allocator is taken out of the picture*.

## Reproduction

```bash
cargo build -p bench-rp2350-m33-stark --release
scp target/thumbv8m.main-none-eabihf/release/bench-rp2350-m33-stark \
    pid-admin@10.42.0.30:/tmp/m33bump.elf

picotool load -v -x -t elf /tmp/m33bump.elf
cat /dev/ttyACM0
```
