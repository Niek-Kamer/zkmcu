# 2026-04-24 — M33 STARK BabyBear x Quartic (Karatsuba), bench-core rebaseline

Correction up front: earlier versions of this note labeled the
extension mul as "schoolbook 16-mult". Thats wrong. `zkmcu-babybear`
has had the 9-mult Karatsuba + sparse `mul_by_W = 11` version as the
default code path since phase 3.3 commit `b794d20`. So this run
measures Karatsuba, not schoolbook. The `-q-kara` suffix pre-refactor
TOML is the matching baseline.

Doesnt matter much for M33 specifically since the pre-refactor `-q`
and `-q-kara` M33 numbers were both 124.2 ms — on Thumb-2 the
hardware-pipelined UMULL made schoolbook and Karatsuba cost about
the same. But for RV32 the two differ (136 vs 129 ms), so labeling
matters on the cross-ISA side.

## Headline

**stark_verify (Fibonacci N=1024, BabyBear x Quartic, Karatsuba):
95.63 ms median, −23.02 % vs the 124.22 ms pre-refactor Karatsuba
baseline.**

Nothing about the verifier or the extension-mul algorithm changed.
No winterfell patches, no field.rs edits, no allocator swap. Just
the `measure_cycles(|| verify(...))` closure wrapping and some
routine dep-list cleanup in the binary's Cargo.toml. Both the pre-
and post-refactor builds compile through the same LTO passes.

## Why this happened (working hypothesis)

BabyBear's `ExtensibleField<4>::mul` does 9 base multiplications +
3 sparse-W multiplies + a bunch of adds/subs. Thumb-2 has ~13
usable GPRs. The hot loop was at the edge of spilling.

Best guess is that shifting `cloned` and `public` captures onto the
closure's implicit stack slot freed up one register in the
extension-mul path, wich flipped at least one spill back to being
register-resident.

Cross-ISA data backs this up: RV32 Karatsuba (31 GPRs, much more
register slack) moved from 129.05 → 128.02 ms, basically flat. If
the effect were icache-layout or codegen-global, both ISAs would
have moved similarly. The register-dense ISA seeing no change and
the register-tight ISA seeing a 23 % win is exactly what a
register-allocation-specific hypothesis predicts.

Still a hypothesis. Actual confirmation wants a `cargo asm` diff of
`<BabyBear as ExtensibleField<4>>::mul` old-vs-new, wich I havent
done yet. Its a win either way so not urgent.

## Phase 3.3 story update — this is the big one

The original phase 3.3 headline was: **"BabyBear × Quartic-Karatsuba
narrows the cross-ISA gap to 1.04x"** (129.05 RV32 / 124.22 M33 =
1.039x). This was the cleanest cross-ISA result in the whole project
and the main BabyBear-related lemma worth pitching.

Under bench-core: **1.339x** (128.02 / 95.63). Not narrow anymore.
The refactor unlocked a register-allocation win on M33 that didnt
transfer to RV32, so M33 pulled ahead. The "BabyBear narrows the
cross-ISA gap" claim is now M33-only noise.

Updated BabyBear/Goldilocks ratio table (matched Karatsuba and
matched allocator on both sides, all under bench-core):

| | Pre-refactor | Post-refactor |
|---|---:|---:|
| M33 BabyBear-Kara / M33 Goldilocks | 1.66x (+66 %) | **1.30x (+30 %)** |
| RV32 BabyBear-Kara / RV32 Goldilocks | 1.15x (+15 %) | **1.19x (+19 %)** |

BabyBear × Quartic-Karatsuba **still does not beat Goldilocks ×
Quadratic at 95-bit conjectured security.** But the margin shifted
meaningfully on M33. Anyone citing the "+66 %" figure should pull
the new number; anyone citing the 1.04x cross-ISA figure should
pull the new 1.34x.
