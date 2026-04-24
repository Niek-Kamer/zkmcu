#import "/research/lib/template.typ": *

#show: paper.with(
  title: "Hand-written Karatsuba helps Hazard3 RV32 but not Cortex-M33",
  authors: ("zkmcu",),
  date: "2026-04-24",
  kind: "postmortem",
  abstract: [
    Phase 3.3 STARK verify with BabyBear base field + Quartic extension landed at 124 ms on M33 and 137 ms on RV32, 66 % and 22 % slower than the Goldilocks-Quadratic baseline. I replaced the 16-multiplication schoolbook `ExtensibleField<4>::mul` with a 9-multiplication Karatsuba implementation plus sparse `mul_by_W = 11`, a 37 % reduction in Mont-muls per extension multiply. Prediction: ~45 ms gain on M33. Reality: zero on M33, −5.5 % on RV32. Root cause is asymmetric: LLVM auto-CSE and pipelined UMAAL on M33 already absorbed the schoolbook overhead.
  ],
)

= Measurement

Phase 3.3 STARK verify, Fibonacci-1024, TLSF heap, 150 MHz:

#table(
  columns: (auto, auto, auto, auto),
  align: (left, right, right, right),
  stroke: 0.4pt + luma(200),
  [*ISA*], [*Schoolbook*], [*Karatsuba*], [*Delta*],
  [Cortex-M33],   [124.21 ms], [124.22 ms], [+12 µs (within noise)],
  [Hazard3 RV32], [136.64 ms], [129.05 ms], [−7.59 ms (−5.5 %)],
)

On M33 the 37 % Mont-mul reduction was invisible. On Hazard3 it showed up as a clean ~5 % wall-clock gain *and* the tightest variance in the whole project so far (std_dev / mean = 0.053 %).

= Root cause

Two effects compound differently on each core.

+ *LLVM auto-CSE on the schoolbook.* At `opt-level=s, lto=fat` the compiler can see common sub-products across the 16 pairwise multiplications. `a[0] * b[0]` appears in three of the four output coefficients; the compiler folds these automatically. The "savings" my hand-written Karatsuba claims are partially already taken before I rewrite anything. On M33 this fold is good enough; on Hazard3 the compiler's scheduling is weaker and the folding doesn't happen as cleanly.
+ *Pipelined UMAAL/UMULL vs in-order MUL.* Cortex-M33 with DSP extensions issues `UMULL` in a pipelined fashion, so extra Mont-muls overlap with surrounding work. Hazard3's minimal integer pipeline pays a full cycle per `MUL` and has no ILP headroom. Every saved mult is a saved cycle.

Together: the M33 compiler + pipeline absorb schoolbook overhead for free; Hazard3 gets no such gift, so hand-written algorithmic improvements land there.

= Fix / next lever

Nothing to "fix" on M33. The schoolbook was already well-optimised for that core. For RV32 the Karatsuba win is real but bounded. Further gains on Hazard3 need a different kind of lever: Frobenius-free extension inversion, sparse intermediate representation, or a different extension degree entirely.

= Rule

When you're comparing Cortex-M33 and Hazard3 RV32 at the same clock, don't assume a hand-written algorithmic improvement will land the same on both ISAs. Pipelined ARM + LLVM scheduling often take the optimization for free at `opt-level=s, lto=fat`; in-order RV32 needs the source-level restructuring to see the win. Measure separately, expect asymmetric results.

Ofcourse this is also a methodology finding: a cross-ISA ratio is allocator-sensitive *and* compiler-scheduling-sensitive. Reproducing any cross-ISA number in this project requires pinning both the allocator and the rustc/LLVM version. Any claim otherwise is partially measuring the toolchain, not the workload.
