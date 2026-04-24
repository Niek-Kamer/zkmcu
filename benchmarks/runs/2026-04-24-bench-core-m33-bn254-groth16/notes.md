# 2026-04-24 — M33 BN254 Groth16, bench-core rebaseline

Yeah so this is the BN254 Groth16 number after the `bench-core` refactor
landed (commit `98c7af5`). Same silicon, same dep versions, same UMAAL
asm, same SRAM-placed `mul_reduce_armv8m`. The only thing that moved is
that the firmware now pulls shared USB / clock / heap-tracking / stack
painting helpers from `crates/bench-core` instead of copy-pasting them
per binary.

## Headline

**Groth16 verify (x^2 = y): 642 ms median, +0.30 % vs the
pre-refactor 641 ms baseline. Noise.**

Scaling results follow the same pattern as before: square → 642 ms,
squares-5 → 648 ms (+6 ms for 4 extra IC slots = ~1.5 ms per IC point,
matches prior runs), semaphore depth-10 → 761 ms.

## What's the same

- `heap_peak` = 81888 B, byte-identical to the previous M33 BN run
- `stack_peak` = 15724 B, byte-identical
- UMAAL KAT passes 256/256 in 12015 us, identical to prior runs

## What's different (vs pre-refactor)

The `measure_cycles(|| verify(...))` closure wrapping and the extra
`bench-core` call-graph layer cost zero measurable cycles in the verify
hot path. LTO inlined everything, DWT reads sit right next to the
`pairing_batch` call like they did before.

## Notes

- Only 4 iterations captured from the serial log. Medians are a touch
  noisier than the 60-sample baseline, but min-max span is 22897 cycles
  (0.024 %) so confidence is high
- USB mid-run truncation on iter 1 boot line happens in both pre- and
  post-refactor builds, USB host race after ~640 ms no-poll window
- Cross-ISA reference: RV32 BN sibling lands at 1362 ms, heap_peak
  81888 B (same — first time we can confirm byte-identical heap
  footprint across both ISAs for BN)
