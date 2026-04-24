#import "/research/lib/template.typ": *

// Phase 3.3 — close-out report for the BabyBear + Quartic experiment.
// Four measurements: Goldilocks-Q baseline from 3.2.z, BabyBear-Q
// schoolbook, BabyBear-Q Karatsuba, on both M33 and RV32.

#let gold_m33   = toml("/benchmarks/runs/2026-04-24-m33-stark-fib-1024-q-tlsf/result.toml")
#let gold_rv32  = toml("/benchmarks/runs/2026-04-24-rv32-stark-fib-1024-q-tlsf/result.toml")
#let bb_sch_m33 = toml("/benchmarks/runs/2026-04-24-m33-stark-fib-1024-babybear-q/result.toml")
#let bb_sch_rv32 = toml("/benchmarks/runs/2026-04-24-rv32-stark-fib-1024-babybear-q/result.toml")
#let bb_kar_m33 = toml("/benchmarks/runs/2026-04-24-m33-stark-fib-1024-babybear-q-kara/result.toml")
#let bb_kar_rv32 = toml("/benchmarks/runs/2026-04-24-rv32-stark-fib-1024-babybear-q-kara/result.toml")

#show: paper.with(
  title: "BabyBear × Quartic on RP2350: a negative latency result wich collapses the cross-ISA gap",
  authors: ("zkmcu",),
  date: "2026-04-25",
  kind: "report",
  abstract: [
    Phase 3.3 tested the small-field STARK hypothesis on the same
    silicon the phase-3.2 allocator matrix closed on: Raspberry Pi
    Pico 2 W, Cortex-M33 + Hazard3 RV32 both at 150 MHz, Fibonacci
    N=1024, 95-bit conjectured security. The hypothesis was that
    `BabyBear` (31-bit, fits a `u32` register natively) should beat
    `Goldilocks` (64-bit, emulated on 32-bit hardware) at matched
    security. To hit 95-bit with BabyBear we forked
    `novifinancial/winterfell` v0.13.1 and added
    `FieldExtension::Quartic` plus a `QuartExtension<B>` wrapper,
    since 62-bit extension caps conjectured security around 50-bit.
    Result: *BabyBear × Quartic loses to Goldilocks × Quadratic on
    both ISAs even after hand-written Karatsuba and sparse mul_by_W
    optimisations* (+66 % on M33, +15 % on Hazard3). But the
    cross-ISA ratio M33-vs-RV32 drops from 1.506× (Goldilocks-Q) to
    1.039× with BabyBear-Karatsuba, wich is the tightest ISA
    levelling in the whole project. Variance record too: 0.053 %
    std-dev on Hazard3, the cleanest timing profile measured on
    RP2350 silicon in zkmcu so far. Negative latency headline,
    positive cross-ISA and variance headlines.
  ],
)

= Context

Phase 3.2 closed with the allocator matrix report: three allocators,
same field + extension (Goldilocks × Quadratic), cross-ISA ratios
between 1.21× (BumpAlloc) and 1.51× (TlsfHeap). Production recommendation
was TlsfHeap at 74.65 ms on M33 / 112.40 ms on Hazard3.

Phase 3.3 opens a different axis: not "what allocator", but "what
field". The community pitch is that small fields like `BabyBear`
($p = 15 dot 2^(27) + 1$) and `Mersenne-31` ($p = 2^(31) - 1$) are
a natural fit for 32-bit MCUs wich can do `u32 * u32 -> u64` in a
single hardware multiplier op, where `Goldilocks`
($p = 2^(64) - 2^(32) + 1$) emulates `u128` arithmetic throughout.

The question this report answers: *does a 31-bit base field actually
beat a 64-bit base field for STARK verify on a 32-bit MCU, at matched
95-bit conjectured security?*

The answer is no. But the run uncovered two other findings that matter
for anyone doing cross-ISA MCU benchmarks.

= The architectural detour: why we forked winterfell

BabyBear × Quadratic is only 62 bits of extension, wich caps
out-of-domain soundness at ~50 bits. Hitting 95-bit on BabyBear needs
Quartic (124 bits) at minimum. Winterfell 0.13.1 ships `Quadratic`
and `Cubic` variants only: the `FieldExtension` enum has no `Quartic`,
and there's no `QuartExtension<B>` wrapper type in `winter-math`. So
before we could measure anything, we needed to either fork winterfell
or switch libraries entirely.

Forked. `Niek-Kamer/winterfell` at upstream `v0.13.1`, vendored into
`vendor/winterfell` as a git submodule, path-patched via
`[patch.crates-io]` for every `winter-*` subcrate. Changes:

- `FieldExtension::Quartic = 4` added to the enum + `TryFrom<u8>` +
  `degree()` + serialization round-trip.
- `winter-math/src/field/extensions/quartic.rs` with the
  `QuartExtension<B: ExtensibleField<4>>(B, B, B, B)` wrapper type
  plus full `FieldElement`, `ExtensionOf<B>`, serialization, Randomizable,
  all arithmetic operator impls.
- `Air::BaseField` and `Prover::BaseField` bound tightened to
  `StarkField + ExtensibleField<2> + ExtensibleField<3> + ExtensibleField<4>`.
- Stub `ExtensibleField<4>` with `is_supported() -> false` added to
  `f62`, `f64`, `f128` so the existing stock fields compile against
  the tightened bound. Same pattern f128 already uses for Cubic.
- `FieldExtension::Quartic => QuartExtension<Self::BaseField>` match
  arms in `winter-prover` and `winter-verifier` dispatch.

All 280+ of winterfell's own tests still pass. The fork is architecturally
additive, no Quadratic or Cubic behaviour changes. ~4 hours of
engineering, not the 4-5 days I first sized the work at, because most
of the plumbing is generic in the extension degree already.

On our side, `crates/zkmcu-babybear/` holds the base field + extension
impl (Montgomery form with $R = 2^(32)$, irreducible polynomial
$x^4 - 11$), and
`crates/zkmcu-verifier-stark/src/fibonacci_babybear.rs` is the AIR port.
Firmware variants are behind a `babybear` cargo feature so the
phase-3.2 baseline binaries stay byte-reproducible.

= Numbers

All six measurements: Fibonacci AIR N=1024, TLSF allocator, Blake3-256
hash, ProofOptions `(32 queries, blowup 8, 0 grinding, FRI folding 8)`
targeting 95-bit conjectured security. Only the field + extension
degree + extension-mul implementation vary.

== Cortex-M33

#compare-table(
  ("Measure", "Goldilocks × Q", "BabyBear × Q (schoolbook)", "BabyBear × Q (Karatsuba)"),
  (
    ([Iterations], [20], [15], [18]),
    ([Median verify], [*74.65 ms*], [124.21 ms], [124.22 ms]),
    ([Min-max variance], [0.343 %], [0.254 %], [0.265 %]),
    ([Std-dev variance], [*0.081 %*], [0.082 %], [0.072 %]),
    ([Heap peak], [93.5 KB], [*91.4 KB*], [*91.4 KB*]),
    ([Stack peak], [5.6 KB], [5.3 KB], [*5.3 KB*]),
    ([Δ vs Goldilocks], [-], [+66.4 %], [+66.4 %]),
  ),
)

== Hazard3 RV32

#compare-table(
  ("Measure", "Goldilocks × Q", "BabyBear × Q (schoolbook)", "BabyBear × Q (Karatsuba)"),
  (
    ([Iterations], [23], [10], [17]),
    ([Median verify], [*112.40 ms*], [136.64 ms], [129.05 ms]),
    ([Min-max variance], [0.411 %], [0.177 %], [0.191 %]),
    ([Std-dev variance], [0.110 %], [0.058 %], [*0.053 %*]),
    ([Stack peak], [5.5 KB], [5.2 KB], [*5.2 KB*]),
    ([Δ vs Goldilocks], [-], [+21.6 %], [+14.8 %]),
  ),
)

== Cross-ISA ratio

#compare-table(
  ("Configuration", "M33", "RV32", "Ratio"),
  (
    ([Goldilocks × Q, TLSF (3.2.z baseline)], [74.65 ms], [112.40 ms], [1.506×]),
    ([BabyBear × Q schoolbook, TLSF], [124.21 ms], [136.64 ms], [1.100×]),
    ([*BabyBear × Q Karatsuba, TLSF*], [*124.22 ms*], [*129.05 ms*], [*1.039×*]),
  ),
)

This last row is tighter than any phase-3.2 allocator variant.
BumpAlloc hit 1.21×, LlffHeap hit 1.33×, TlsfHeap hit 1.51×. Field
choice is a bigger cross-ISA levelling lever than allocator choice.

= Findings

== 1. BabyBear does not beat Goldilocks at 95-bit on either ISA

The base-field advantage of BabyBear (single `UMULL` / `MUL` on M33 /
Hazard3, versus emulated 64-bit on either) is real but bounded. The
Quartic extension kills it:

- `ExtensibleField<4>::mul` schoolbook costs 16 base multiplications
  + 3 `W × X` multiplies. Karatsuba brings that to 9 + 3. Goldilocks
  `ExtensibleField<2>::mul` is 3 base multiplies total.
- Extension inversion costs 3 Frobenius + 3 extension multiplies for
  Quartic (norm-via-orbit-product trick), versus 1 Frobenius + 1
  extension multiply for Quadratic.
- FRI folding at degree 4 does 2× the base-field work per fold round
  as degree 2, across the 13 folding rounds.

Per-op BabyBear speed is ~3× Goldilocks on a good day. Extension-level
multiplicity is 3-4× against us. Net: worse, not better.

*Rule*: at a fixed STARK security target, field choice isn't a free
variable; the extension degree needed to hit that target is coupled.
A 31-bit field is only a win at security targets where Quadratic is
sufficient (i.e. below ~50-bit conjectured on BabyBear, below ~60-bit
on Mersenne-31). For 95-bit production-grade security you pay the
extension cost, and the trade goes the other way.

== 2. Karatsuba extension-mul helps Hazard3 but not M33

Hand-written 9-mult Karatsuba `ExtensibleField<4>::mul` saved 5.55 %
wall-clock on Hazard3. On Cortex-M33 it saved 12 µs, within noise:

#compare-table(
  ("ISA", "Schoolbook", "Karatsuba", "Δ"),
  (
    ([Cortex-M33], [124.21 ms], [124.22 ms], [*+12 µs (noise)*]),
    ([Hazard3 RV32], [136.64 ms], [129.05 ms], [*-7.59 ms (-5.55 %)*]),
  ),
)

Two effects together explain the asymmetry. LLVM at `opt-level=s` +
`lto=fat` CSEs the 16-mult schoolbook into a form close to what
Karatsuba does by hand, because the 16 products share many
sub-expressions (e.g. `a[0] * b[0]` feeds three of the four output
coefficients). And Cortex-M33's pipelined `UMULL` + DSP extensions
+ BTB absorb the remaining extra multiplies via instruction-level
parallelism. Hazard3 has neither the compiler scheduling gift nor
the ILP headroom, so every Mont-mul cut is a real cycle saved.

*Rule*: cross-ISA optimisation doesn't generalise. A hand-optimised
extension-arithmetic routine that helps Hazard3 can easily do nothing
on M33, or vice versa. Measure each ISA separately. Details in
`.claude/findings/2026-04-24-karatsuba-isa-asymmetric.md`.

== 3. Cross-ISA gap collapses to 1.04× with BabyBear-Karatsuba

Under Goldilocks × Quadratic the Hazard3-vs-M33 gap is 1.506×, caused
mostly by Hazard3's tax on emulated `u64` arithmetic (pairs of `MUL` +
`MULHU` synthesising each 64-bit multiply). BabyBear removes that tax
because a 31-bit field fits a single `u32` multiply natively on both
cores.

This is probably the most ISA-levelling result in the whole zkmcu
project. The phase-3.2.z allocator matrix swung cross-ISA from 1.21×
to 1.51× depending on allocator pick. Swapping from Goldilocks to
BabyBear (+ Karatsuba extension mul) drops it to 1.039×, below every
allocator variant.

For a developer picking between Cortex-M33 and Hazard3 deployment at
the same clock, at 95-bit STARK security on this workload: your field
choice matters more than your ISA choice.

== 4. Variance record: 0.053 % std-dev on Hazard3

`BabyBear × Quartic Karatsuba` on Hazard3 landed at std-dev / mean =
0.053 %, tighter than any previous measurement:

#compare-table(
  ("Config", "Std-dev variance"),
  (
    ([BumpAlloc (3.2.y, Goldilocks)], [0.076 %]),
    ([TlsfHeap (3.2.z, Goldilocks)], [0.081 %]),
    ([BabyBear × Q Karatsuba (this run, RV32)], [*0.053 %*]),
  ),
)

Likely cause: BabyBear's Montgomery reduction is branch-free on
u32-native Hazard3 (one `MUL` + one conditional subtract), and the
Karatsuba structure has fewer nested conditional branches than the
schoolbook form. Fewer mispredict opportunities on a minimal
predictor.

For side-channel-sensitive firmware on Hazard3, BabyBear-Karatsuba
is the most deterministic timing profile in the project, even at
the cost of ~15 % median latency over Goldilocks.

= What this means for the production story

Phase-3.2.z recommended `TlsfHeap` + Goldilocks × Quadratic as the
production configuration for 95-bit STARK verify on RP2350-class
silicon. Phase 3.3 does *not* change that recommendation: Goldilocks
× Quadratic still wins on median latency on both ISAs.

BabyBear × Karatsuba becomes interesting in two narrower deployment
shapes:

#compare-table(
  ("Deployment shape", "Pick"),
  (
    ([Latency-bound hardware wallet on M33], [Goldilocks × Q (74.65 ms)]),
    ([Latency-bound hardware wallet on RV32], [Goldilocks × Q (112.40 ms)]),
    ([Side-channel-sensitive on Hazard3], [BabyBear × Q Karatsuba (variance 0.053 %)]),
    ([Cross-ISA portable SDK], [Either, but BabyBear closes the gap (1.04× vs 1.51×)]),
  ),
)

= Open tracks

Three threads for phase 3.4 or beyond, in rough order of expected payoff:

- *Plonky3 circle-STARK over Mersenne-31.* A different protocol where
  extension arithmetic is designed around 32-bit hardware. The only
  realistic path to "small-field STARK beats Goldilocks-Quadratic on
  MCU at 95-bit". Significant port cost, estimated 1-2 weeks.
- *Bigger AIRs ($N = 2^16$, $N = 2^18$).* Phase-3.2 skipped this.
  Extension-field work scales linearly with trace length; hashing
  scales sub-linearly. Possible that BabyBear's relative cost improves
  at larger AIRs where extension work dominates less.
- *Hand-asm UMULL for BabyBear `mont_reduce`* on M33, same playbook as
  the Groth16 phase-2 work that went 988 ms → 641 ms. But Karatsuba
  already showed extension-mul isn't the M33 bottleneck, so the
  expected payoff is small.

The cross-ISA and variance results stand regardless of wich thread
gets picked up. They're archived in
`benchmarks/runs/2026-04-24-{m33,rv32}-stark-fib-1024-babybear-q{,-kara}`
and the findings at
`.claude/findings/2026-04-24-karatsuba-isa-asymmetric.md` +
`2026-04-24-babybear-quartic-regresses.md`.

#v(1em)

_Winterfell fork_: `github.com/Niek-Kamer/winterfell` (branch `main` at
upstream v0.13.1 + Quartic patch).

_BabyBear base + extension_: `crates/zkmcu-babybear/`.

_AIR_: `crates/zkmcu-verifier-stark/src/fibonacci_babybear.rs`.

_Host-side prover_: `crates/zkmcu-host-gen/src/stark_babybear.rs`,
invoked via `cargo run -p zkmcu-host-gen --release -- stark-babybear`.

_Firmware_: `crates/bench-rp2350-{m33,rv32}-stark` with the `babybear`
cargo feature. Build targets `just build-m33-stark-bb` and
`build-rv32-stark-bb`.
