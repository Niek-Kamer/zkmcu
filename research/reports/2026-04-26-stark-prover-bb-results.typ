#import "/research/lib/template.typ": *

#let m33 = toml("/benchmarks/runs/2026-04-26-m33-stark-prover-bb/result.toml")
#let rv32 = toml("/benchmarks/runs/2026-04-26-rv32-stark-prover-bb/result.toml")
#let ph4  = toml("/benchmarks/runs/2026-04-26-m33-stark-prover-fib/result.toml")

#show: paper.with(
  title: "Phase 5: BabyBear + Quartic STARK prover on RP2350 — 95-bit security at 148 ms, and wich ISA is actually faster",
  authors: ("zkmcu",),
  date: "2026-04-26",
  kind: "report",
  abstract: [
    Phase 4 proved that a STARK prover fits on a microcontroller. Phase 5
    asks: can it run at production-grade security without blowing the
    budget? Answer: yes, barely. Switching from Goldilocks + no extension
    (~32-bit security) to BabyBear + Quartic extension (~95-bit security)
    costs only *+10 % prove time* on Cortex-M33 (148 ms vs 134 ms) and
    actually *reduces heap by 52 KB* (253 KB vs 306 KB). Proof size is
    essentially the same (6 872 B vs 6 668 B). The cross-ISA gap
    (M33 vs Hazard3 RV32) narrowed from 1.55× in Phase 4 to *1.42×* —
    but not because BabyBear field arithmetic is cheaper on RV32, wich
    was the hypothesis. The gap stayed large because Blake3 hashing
    dominates both prove and verify time, and Blake3 runs 1.4× faster
    on M33 regardless of field choice. N=256 is still the SRAM ceiling
    on this chip at blowup=4.
  ],
)

= Context

Phase 4 closed with Goldilocks `FieldExtension::None` at ~32-bit
conjectured security. That was intentional — it was a feasibility
run, not a production one.

Phase 5 switches two things at once: base field (Goldilocks 64-bit →
BabyBear 31-bit) and extension (None → Quartic). The winterfell fork
already had `FieldExtension::Quartic` from phase 3.3. The new crate
is `bench-rp2350-m33-stark-prover-bb` (M33) and
`bench-rp2350-rv32-stark-prover-bb` (RV32). Both use the same
`zkmcu-babybear` field and the same Fibonacci AIR as Phase 4.

N=256 was kept fixed. N=512 was attempted and immediately OOM'd
(~496 KB heap needed, 384 KB available). The ceiling did not move.

= Results: M33, BabyBear + Quartic

#compare-table(
  ("Metric", "Phase 4 (Goldilocks+None)", "Phase 5 (BabyBear+Quartic)", "Delta"),
  (
    ([Security (conjectured)], [~32 bit],    [~95 bit],    [+63 bit]),
    ([Prove (ms)],             [134],        [*148*],      [+10.4 %]),
    ([Verify (ms)],            [19.4],       [29],         [+49 %]),
    ([Heap peak (KB)],         [299],        [*248*],      [−17.5 %]),
    ([Proof size (B)],         [6 668],      [6 872],      [+3 %]),
    ([Stack peak (B)],         [4 448],      [4 672],      [+0.5 %]),
  ),
)

*Prove overhead is +10 % for three times the security bits.* That is a
good tradeoff. The heap actually shrinks because BabyBear elements are
4 bytes vs 8 bytes for Goldilocks — the LDE matrix is half the size.
Verify costs more because the Quartic extension forces a bigger
composition polynomial (4 columns instead of 1).

= Cross-ISA: M33 vs RV32

#compare-table(
  ("Metric", "Cortex-M33", "Hazard3 RV32", "Ratio (RV32 / M33)"),
  (
    ([Prove (ms)],      [148], [211],  [*1.42×*]),
    ([Verify (ms)],     [29],  [40],   [*1.39×*]),
    ([Heap peak (KB)],  [248], [248],  [1.00×]),
    ([Proof size (B)],  [6 872], [6 872], [1.00×]),
  ),
)

The hypothesis was that the gap would collapse from 1.55× (Phase 4) to
~1.07×, because BabyBear field multiplication is 32×32→64 on both ISAs
— symmetric, no UMAAL advantage for M33.

That did not happen. The gap only moved from 1.55× to 1.42×.

The reason becomes obvious when you compare prove and verify ratios:
both are ~1.4×. Verify is almost pure Blake3 Merkle path work — no
field arithmetic at all. If verify is 1.39× slower on RV32, then
1.39× is the Blake3 floor. Prove adds a bit more on top (1.42×) from
field ops, but the contribution is tiny compared to the hash bottleneck.

*Blake3 runs ~1.4× faster on M33 than on Hazard3, regardless of field
choice.* Changing fields does not help.

= The SRAM ceiling

N=512 at blowup=4 needs ~496 KB heap. The RP2350 has 512 KB SRAM total,
wich leaves ~16 KB for stack, .bss, and runtime overhead. It OOM'd
immediately. The ceiling is still N=256 on this chip.

Breaking it requires either:
- A field-friendly hash (Poseidon or Rescue) to replace Blake3 in the
  commitment scheme — Merkle trees over a field hash skip the
  byte-oriented compression entirely.
- External PSRAM on the board (RP2350 supports it via QSPI; the Pico 2 W
  does not populate it).

= Summary

#compare-table(
  ("", "Phase 4", "Phase 5"),
  (
    ([Field],          [Goldilocks (64-bit)], [BabyBear (31-bit)]),
    ([Extension],      [None],                [Quartic]),
    ([Security],       [~32 bit],             [~95 bit]),
    ([M33 prove],      [134 ms],              [148 ms]),
    ([RV32 prove],     [208 ms],              [211 ms]),
    ([ISA gap],        [1.55×],               [1.42×]),
    ([Heap (M33)],     [299 KB],              [248 KB]),
    ([Proof size],     [6 668 B],             [6 872 B]),
  ),
)

Production-grade security on a \$4 chip at 148 ms. The ISA gap is
real and stays real — it is a Blake3 problem, not a field problem.
