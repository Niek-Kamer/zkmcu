#import "/research/lib/template.typ": *

#show: paper.with(
  title: "opt-level=3 regressed Groth16 verify by 14.7 %",
  authors: ("zkmcu",),
  date: "2026-04-23",
  kind: "postmortem",
  abstract: [
    Compiler-flag audit, first optimization lever on the 988 ms BN254 Groth16 verify baseline. Workspace `[profile.release]` had `opt-level = "s"`, wich on a benchmark crate looked like an obvious miss. Flipped to `3`, rebuilt, measured. Slower, not faster: +14.7 % on verify and +67 % on `.text`. Reverted. Direction is a library/target-specific fact, not a durable rule.
  ],
)

= Measurement

Single Groth16 verify on the `square` test vector, median over 4 iterations:

#table(
  columns: (auto, auto, auto),
  align: (left, right, right),
  stroke: 0.4pt + luma(200),
  [*Profile*], [*groth16_verify (square)*], [*.text size*],
  [`opt="s"` (baseline)], [988 ms], [73 KB],
  [`opt=3`],              [1 133 ms (+14.7 %)], [123 KB (+67 %)],
)

Regression reproduced on every sub-benchmark: `sq5` ~1 160 ms, `semaphore-depth-10` ~1 645 ms, single pairing ~576 ms. Iteration-to-iteration spread was ±1 ms, so this is not noise.

= Root cause

Two effects compound.

+ *XIP cache thrash.* RP2350 executes from QSPI flash through a 16 KB on-chip cache. 73 KB of hot Miller-loop code fits (with some eviction); 123 KB doesn't fit at all. Every loop iteration pays cache-miss latency on the code path that is 60 %+ of the verify.
+ *Register spilling extension.* 32-bit ARM has 12 usable GPRs. 256-bit Montgomery mul is already spill-heavy at opt=s (226 loads, 162 stores per `U256::mul` call). Aggressive inlining at opt=3 extends the live range of temporaries, wich increases spill count, wich means more memory traffic on a workload that is already memory-bound. The extra spill instructions are *also* the extra code busting the cache, so the two effects reinforce each other.

= Fix

Reverted `Cargo.toml` to `opt-level = "s"`. Full regressed run is preserved at `benchmarks/runs/2026-04-23-m33-opt-level-3/` with raw log + TOML + notes, so the number is reproducible if anyone wants to double-check.

= Rule

For 32-bit ARM MCU crypto workloads on a chip with small XIP cache, `opt-level=3` is not a safe assumption. Ofcourse the x86/desktop intuition doesn't transfer here: size-optimised heuristics happen to fit the workload shape better. Corollary: compiler-flag tweaks are unlikely to move this baseline by more than a few percent in either direction. The real levers are algorithmic (G2 precomputation, GLV, FE chain) and instruction-level (UMAAL asm Montgomery mul), not compiler heuristics.

I am *not* promoting this to a durable project rule because the direction (not just the magnitude) is a one-time fact for this specific library + target combination. Bigger XIP cache, a different chip, or a hand-rewritten Fp layer could flip the sign. Future experiments should still try `opt-level` values, just not assume a win.
