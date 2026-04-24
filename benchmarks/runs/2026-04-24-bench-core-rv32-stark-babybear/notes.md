# 2026-04-24 — RV32 STARK BabyBear x Quartic (Karatsuba), bench-core rebaseline

Correction up front: earlier versions of this note labeled the
extension mul as "schoolbook 16-mult". Thats wrong. `zkmcu-babybear`
has had the 9-mult Karatsuba + sparse `mul_by_W = 11` version as
the default code path since phase 3.3 commit `b794d20`. The
`-q-kara` pre-refactor TOML (129.05 ms) is the matching baseline,
not the `-q` one (136.64 ms, schoolbook).

## Headline

**stark_verify (Fibonacci N=1024, BabyBear x Quartic, Karatsuba):
128.02 ms median, −0.80 % vs the 129.05 ms pre-refactor Karatsuba
baseline.**

Essentially flat. Within run-to-run noise. The same refactor that
dropped M33 BabyBear by 23 % barely touched RV32, wich is the
cross-ISA asymmetry story.

## Register-pressure hypothesis, now stronger

If the bench-core refactor had caused a global codegen shift
(function layout, icache, inlining), both ISAs would have moved by
comparable magnitudes. On matched Karatsuba code: M33 moved from
124.22 → 95.63 ms (-23.0 %), RV32 moved from 129.05 → 128.02 ms
(-0.80 %). The 30x magnitude gap between ISA deltas is way outside
any plausible uniform-codegen explanation.

What fits: Thumb-2's 13 usable GPRs put the Karatsuba extension-mul
inner loop right at the spill boundary. Closure-wrapping shifted
where captures live in the frame and freed a register in the hot
loop. RV32IMAC has 31 GPRs, was never register-tight, so nothing
to unlock there.

## Phase 3.3 story update — RV32 side

The original phase 3.3 headline framed BabyBear-Karatsuba as the
config that narrows the cross-ISA gap:

| Config | M33 | RV32 | RV32/M33 |
|---|---:|---:|---:|
| Goldilocks x Quadratic (pre-refactor) | 74.65 | 112.40 | 1.506x |
| BabyBear x Quadratic schoolbook (pre-refactor) | 124.21 | 136.64 | 1.100x |
| **BabyBear x Quartic-Karatsuba (pre-refactor)** | **124.22** | **129.05** | **1.039x** ← was the headline |
| BabyBear x Quartic-Karatsuba (bench-core) | 95.63 | 128.02 | **1.339x** ← now this |

The 1.04x narrowing was a pre-refactor artifact. Under bench-core
the cross-ISA ratio widened back to 1.34x, which is bigger than
pre-refactor Goldilocks (1.47x-1.51x is a fair comparison since
Goldilocks also saw small wins from the refactor).

So the phase 3.3 cross-ISA story needs a qualifier: "narrowed to
1.04x under the specific pre-refactor register allocation that was
shipping with zkmcu-babybear commit b794d20, an artifact that
bench-core's closure wrapping ended up erasing on M33."

## Heap + variance

`heap_peak = 91_362 B`, byte-identical to M33 BabyBear-Kara
sibling. First time measured on RV32. Variance
(max-min)/median = 0.23 %, tight.
