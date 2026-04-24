# 2026-04-24 — M33 STARK BabyBear x Quartic, bench-core rebaseline

This is the headline result of the bench-core refactor. Not the
refactor's main goal, just a structural-cleanup thing that
accidentally kicked a cached ~30 ms off the number.

## Headline

**stark_verify (Fibonacci N=1024, BabyBear x Quartic, schoolbook
16-mult): 95.6 ms median, −23.0 % vs 124.2 ms pre-refactor.**

Nothing about the verifier changed. No winterfell patches, no
extension-mul changes, no allocator swap. Just a `measure_cycles(||
verify(...))` closure wrapping the verify call instead of a pair of
inline `DWT::cycle_count` reads, and some routine dep-list cleanup
in the binary's Cargo.toml. Both the pre- and post-refactor builds
compile through the same LTO passes.

## Why this happened (working hypothesis)

BabyBear's `ExtensibleField<4>::mul` touches 4 base-field limbs per
operand, so the inner loop does more register bookkeeping than
Goldilocks's Quadratic variant. Thumb-2 has ~13 usable GPRs and the
mul was already at the edge. My best guess is that shifting `cloned`
and `public` captures onto the closure's implicit stack slot freed
up one register in the hot extension-mul path, wich flipped a spill
back to being register-resident.

Cross-ISA data backs this up: RV32 (31 GPRs, much more register
slack) saw only −6.2 % on the same BabyBear refactor. If the effect
were icache-layout or codegen-global, both ISAs would've moved
similarly. Register-pressure-specific win fits better.

Still a hypothesis. Actual confirmation wants a `cargo asm` diff of
`fib_verify_babybear::QuartExtension::mul` old-vs-new, wich I havent
done yet. Its a win either way so not urgent.

## Phase 3.3 story update

Updated BabyBear/Goldilocks ratio table:

| | Pre-refactor | Post-refactor |
|---|---:|---:|
| M33 BabyBear / M33 Goldilocks | 1.66x (+66 %) | **1.30x (+30 %)** |
| RV32 BabyBear / RV32 Goldilocks | 1.22x (+22 %) | **1.19x (+19 %)** |

BabyBear × Quartic still does not beat Goldilocks × Quadratic at
95-bit conjectured security. But the margin is less pessimistic than
the original phase 3.3 writeup. Anyone citing the "+66 %" figure
should pull the new number.

Karatsuba variant not re-measured yet — on the to-do.
