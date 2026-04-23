#import "/research/lib/template.typ": *

#let baseline = toml("/benchmarks/runs/2026-04-21-m33-groth16-baseline/result.toml")
#let fork     = toml("/benchmarks/runs/2026-04-23-m33-fork-baseline/result.toml")
#let asm-flash = toml("/benchmarks/runs/2026-04-23-m33-umaal-asm/result.toml")
#let asm-sram = toml("/benchmarks/runs/2026-04-23-m33-umaal-ram-tuned/result.toml")
#let asm-final = toml("/benchmarks/runs/2026-04-23-m33-umaal-nozeroinit/result.toml")

#let pct(before, after) = {
  let p = (before - after) / before * 100.0
  str(calc.round(p, digits: 1)) + "%"
}
#let ms(t) = str(calc.round(t / 1000.0, digits: 1)) + " ms"

#show: paper.with(
  title: "Groth16/BN254 verify on Cortex-M33 with UMAAL Montgomery multiply",
  authors: ("zkmcu",),
  date: "2026-04-23",
  kind: "report",
  abstract: [
    Replacing `substrate-bn`'s 128-bit-digit CIOS Montgomery multiply with a
    hand-written ARMv8-M assembly implementation using UMAAL on 32-bit limbs,
    and placing the function in SRAM instead of XIP flash, drops full
    Groth16/BN254 verify on a Raspberry Pi Pico 2 W (RP2350 Cortex-M33 @
    150 MHz) from #ms(baseline.bench.groth16_verify.us_median) to
    #ms(asm-final.bench.groth16_verify.us_median), a
    #pct(baseline.bench.groth16_verify.us_median, asm-final.bench.groth16_verify.us_median)
    reduction. Same hardware, same toolchain, same test vector, same result
    (`ok=true`).
  ],
)

= The gap LLVM leaves

The baseline report measured #ms(baseline.bench.groth16_verify.us_median) for
a single Groth16 verify on unmodified `substrate-bn` 0.6.0 and flagged
Cortex-M33's DSP-extension UMAAL as the natural next lever. Before writing
asm, it's worth confirming the gap: disassembling the baseline firmware's
4116-byte `U256::mul` finds 145 `UMULL` / `UMLAL` instructions and *zero*
`UMAAL`. LLVM's M-profile instruction selector doesn't pattern-match
`substrate-bn`'s `u128`-based schoolbook onto UMAAL, wich would fuse the
multiply and two carry accumulates into one single-cycle instruction. The
same disassembly shows 226 loads and 162 stores around those 140 wide
muls — evidence of heavy register spilling on a 32-bit ARM whose 12-register
GPR budget is well short of the 16 slots a 256-bit Montgomery multiply
wants live.

An earlier experiment confirmed compiler flags don't help here. Flipping
`opt-level` from `s` to `3` produced a
#pct(asm-flash.bench.groth16_verify.us_median, baseline.bench.groth16_verify.us_median + 50000)
regression instead of a speedup, because the larger output busted the
RP2350's 16 KB XIP instruction cache and inlined more temporaries into the
already-spill-heavy inner loop. The finding is recorded at
`.claude/findings/2026-04-23-opt-level-3-regression.md`. No amount of
`-C target-feature` tweaking was going to summon UMAAL from rustc 1.94.1 /
LLVM 19 on this target. The instruction exists; the selector doesn't reach
for it.

= The replacement

The asm lives in a fork of `substrate-bn` at `vendor/bn` behind a
`cortex-m33-asm` Cargo feature. The feature routes the crate's internal
`mul_reduce` through a new `mul_reduce_armv8m` on `target_arch = "arm"`
and leaves the Rust implementation intact everywhere else, so the fork's
host test suite (32 existing substrate-bn tests plus a pair of fresh
10'000-iteration cross-checks against a u32-limb Rust reference) exercises
the same algorithm the asm implements.

Algorithm: Separated Operand Scanning on 8 × u32 limbs. Phase 1 is an 8×8
schoolbook fully unrolled into 128 UMAAL steps, each of shape
`t[k] := a[i]*b[j] + t[k] + carry`. Phase 2 is 8 Montgomery reduction
rows of identical shape against the modulus, each followed by an ADDS /
ADCS chain that propagates the final carry word through the top of the
16-word accumulator. Register scheduling keeps `by[0..7]` live in
`r4..r11` across all of Phase 1 and swaps in `modulus[0..7]` for Phase 2,
so no operand reloads happen inside either of the two 64-UMAAL inner
loops.

= SRAM execution

The `mul_reduce_armv8m` body is placed in a `.ram_text` section (VMA in
SRAM, LMA in flash) via one addition to the firmware's `memory.x` and a
`#[pre_init]` hook that does the flash-to-RAM copy before cortex-m-rt's
normal bss / data initialization. RP2350's XIP cache is 16 KB; the
firmware's `.text` is 73 KB; moving the single hottest function off the
XIP-backed path reduces cache pressure on everything else in the pairing
code. The function ends up at VMA `0x20000000`, and the linker
auto-inserts a 10-byte long-branch thunk in flash to bridge the 256 MB
address gap to the original call sites.

= Results

The arc, read from the benchmark TOMLs:

#table(
  columns: (auto, auto, auto),
  align: (left, right, right),
  stroke: 0.4pt + luma(200),
  [*Layer*], [*Groth16 verify (square)*], [*Δ vs crates.io*],
  [crates.io substrate-bn 0.6.0 (baseline)],
    [#ms(baseline.bench.groth16_verify.us_median)],
    [0%],
  [Fork of paritytech/bn master (no code change)],
    [#ms(fork.bench.groth16_verify.us_median)],
    [#pct(baseline.bench.groth16_verify.us_median, fork.bench.groth16_verify.us_median)],
  [+ UMAAL `mul_reduce` asm, flash-resident],
    [#ms(asm-flash.bench.groth16_verify.us_median)],
    [#pct(baseline.bench.groth16_verify.us_median, asm-flash.bench.groth16_verify.us_median)],
  [+ SRAM placement and operand-row register scheduling],
    [#ms(asm-sram.bench.groth16_verify.us_median)],
    [#pct(baseline.bench.groth16_verify.us_median, asm-sram.bench.groth16_verify.us_median)],
  [+ zero-init removal and LDM preloads],
    [#ms(asm-final.bench.groth16_verify.us_median)],
    [*#pct(baseline.bench.groth16_verify.us_median, asm-final.bench.groth16_verify.us_median)*],
)

Per-primitive, comparing the final build against the crates.io baseline:

#table(
  columns: (auto, auto, auto, auto),
  align: (left, right, right, right),
  stroke: 0.4pt + luma(200),
  [*Operation*], [*Baseline*], [*With asm*], [*Δ*],
  [BN254 pairing],
    [#ms(baseline.bench.pairing.us_median)],
    [#ms(asm-final.bench.pairing.us_median)],
    [#pct(baseline.bench.pairing.us_median, asm-final.bench.pairing.us_median)],
  [G1 scalar mul (typical)],
    [#ms(baseline.bench.g1_mul.us_typical)],
    [#ms(asm-final.bench.g1_mul.us_typical)],
    [#pct(baseline.bench.g1_mul.us_typical, asm-final.bench.g1_mul.us_typical)],
  [G2 scalar mul (typical)],
    [#ms(baseline.bench.g2_mul.us_typical)],
    [#ms(asm-final.bench.g2_mul.us_typical)],
    [#pct(baseline.bench.g2_mul.us_typical, asm-final.bench.g2_mul.us_typical)],
)

Two scaling circuits confirm the speedup is not specific to the
two-input test: a five-public-input multi-square verify lands at
#ms(asm-final.bench.groth16_verify_sq5.us_median), and a Semaphore
depth-10 verify lands at #ms(asm-final.bench.groth16_verify_semaphore.us_median),
both with `ok=true` across the run.

Variance across the 15-iteration final run is ±0.1 ms on Groth16 verify
and ±0.6 ms on single-pairing, tight enough that none of the
layered improvements are noise.

The G1 scalar-mul drop (−70%) is larger than the pairing drop (−34%)
because the fork's master branch already contained a post-0.6.0
improvement to `G::mul` unrelated to the asm work. Splitting the two
effects: paritytech/bn master vs. 0.6.0 alone moves g1_mul from
#ms(baseline.bench.g1_mul.us_typical) to
#ms(fork.bench.g1_mul.us_typical); the UMAAL asm on top of that moves it
the rest of the way to #ms(asm-final.bench.g1_mul.us_typical).

= Footprint

`mul_reduce_armv8m` is #asm-final.footprint.text_bytes bytes of firmware
`.text` total. The asm function itself takes 2038 bytes and the
compiler-generated `U256::mul` wrapper around it takes 334 bytes — down
from 4116 bytes of pure LLVM output for the same function at the
baseline. `.text` footprint is #(asm-final.footprint.text_bytes / 1024) KB
vs. #(baseline.footprint.text_bytes / 1024) KB at the baseline; the
asm shrank the binary by net 2 KB despite containing a full hand-unrolled
Montgomery multiply.

= What remains

Prior art on BN254 pairings scaled to Cortex-M33 at 150 MHz suggests a
realistic floor around 300–500 ms for a single pairing on a scalar
32-bit core with hand-tuned asm. This work lands at
#ms(asm-final.bench.pairing.us_median) per single pairing, so there
is still roughly 50–150 ms plausibly available. The obvious next levers
are, in descending order of expected return:

- Hand-written asm for `Fq12::cyclotomic_squared`, `Fq12::mul_by_024`,
  and `Fq2::mul`. These are the next three biggest LLVM-generated
  functions in the pairing hot path and they still execute from flash.
  Each is plausibly 3–7% on full verify.
- GLV endomorphism on the G1 scalar multiplication, wich halves the
  effective scalar length.
- A tuned final-exponentiation chain (Fuentes-Castañeda or
  Duquesne-Ghammam) replacing `substrate-bn`'s current chain.

None is an obvious UMAAL-sized lever. Each is its own separate piece of
work; stacked realistically, they plausibly take verify toward the lower
end of the 500–600 ms range.

= Reproduction

All five benchmark runs referenced here have raw logs, result TOMLs, and
reproduction commands under `benchmarks/runs/`. The asm source is in the
fork at `vendor/bn/src/arith.rs` behind the `cortex-m33-asm` feature; the
linker glue is in `crates/bench-rp2350-m33/memory.x` and the boot copy
hook is in the same crate's `src/main.rs`.
