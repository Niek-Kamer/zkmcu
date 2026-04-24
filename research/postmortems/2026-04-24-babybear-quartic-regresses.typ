#import "/research/lib/template.typ": *

#show: paper.with(
  title: "BabyBear × Quartic does not beat Goldilocks × Quadratic at 95-bit security",
  authors: ("zkmcu",),
  date: "2026-04-24",
  kind: "postmortem",
  abstract: [
    Phase 3.3 hypothesis: a 31-bit BabyBear base field fits a `u32` register natively on 32-bit MCUs, wich *should* beat a 64-bit Goldilocks base field that requires emulated `u64` arithmetic. This is the whole argument for the multi-day fork of winterfell to add `FieldExtension::Quartic`. Measurement disconfirms the hypothesis. On RP2350 at 150 MHz, BabyBear × Quartic lost to Goldilocks × Quadratic on both ISAs, +66 % on M33 and +15 % on Hazard3, even after Karatsuba optimisations. The Quartic extension overhead eats the base-field advantage.
  ],
)

= Matched-security framing

BabyBear × Quadratic is 62 bits of extension, wich can't carry 95-bit conjectured soundness (out-of-domain sampling term bounds it at ~50 bits). BabyBear × Quartic is 124 bits, wich roughly matches Goldilocks × Quadratic's 128 bits of extension. The security-equivalent comparison is Quartic vs Quadratic; anything else is not a fair fight.

= Measurement

#table(
  columns: (auto, auto, auto, auto),
  align: (left, right, right, right),
  stroke: 0.4pt + luma(200),
  [*Config*], [*M33*], [*RV32*], [*RV32 / M33*],
  [Goldilocks × Quadratic, TLSF],                [74.65 ms],  [112.40 ms], [1.506×],
  [BabyBear × Quartic, schoolbook, TLSF],        [124.21 ms], [136.64 ms], [1.100×],
  [BabyBear × Quartic, Karatsuba, TLSF],         [*124.22 ms (+66 %)*], [*129.05 ms (+15 %)*], [*1.039×*],
)

Karatsuba closes the RV32 gap (+22 % → +15 %) but not below Goldilocks, and on M33 has zero effect (see 2026-04-24-karatsuba-isa-asymmetric for why).

= Root cause

Quartic-vs-Quadratic extension-degree overhead eats BabyBear's base-field advantage. Specific cost drivers:

+ `ExtensibleField<4>::mul` is 9-16 base mults vs 3 for `ExtensibleField<2>`. Even optimally this is 3× more base mults per extension multiply.
+ `ExtensibleField<4>` inversion uses three Frobenius calls plus three extension multiplies (norm-via-orbit-product); `ExtensibleField<2>` uses one. ~3× more extension-inversion work.
+ FRI folding at degree 4 is 2× the base-field work per fold vs degree 2. Over ~13 fold rounds, that aggregates materially.
+ Extension-element serialisation is the same 16 bytes per element either way, so not a cost driver.

BabyBear's `u32` mul being roughly 3× faster than Goldilocks `u64` mul on M33 is real but bounded. It can't compensate for 3× more extension-level work across multiple structural phases of the verifier.

Karatsuba on `ExtensibleField<4>::mul` was the cheapest optimisation to try. It didn't close the gap because the gap isn't primarily in extension multiplication. It's in FRI folding + extension inversion + downstream overhead, none of wich are addressable by touching `mul` alone.

= The silver lining

BabyBear × Quartic × Karatsuba has *RV32 / M33 = 1.04×*, compared to *1.51×* for Goldilocks × Quadratic. The cross-ISA gap is essentially closed. That's a legitimate, publishable result in its own right: at the cost of +30-66 % wall-clock on M33, you get ISA-parity between ARM and RISC-V on this workload. Useful if your argument is about RISC-V viability, not about fastest-possible verify. See the companion report `2026-04-24-babybear-quartic-cross-isa.typ`.

= Alternatives

There isn't a single code change that flips the end-to-end latency result within winterfell + 95-bit security + Quartic extension. Real alternatives:

- *Accept lower security.* BabyBear × Cubic = 93 bits of extension, caps at ~80-bit conjectured, wich is publishable as "80-bit BabyBear on MCU" with appropriate caveats.
- *Different STARK protocol.* Plonky3's circle-STARK over Mersenne-31 ($p = 2^31 - 1$) is architecturally different and has extension arithmetic designed around 32-bit hardware. Phase 4+ territory.
- *Publish the negative result.* The community has been assuming small-field STARKs win on small processors. This is a clean counterexample at matched security, inside winterfell's extension-degree options. That's a real contribution, just not a speedup.

= Rule

At 95-bit STARK conjectured security on RP2350-class MCUs, a 31-bit base field needs the Quartic extension, and the Quartic overhead eats the base-field-fits-register advantage. BabyBear is *not* a drop-in win on 32-bit MCUs; it loses to Goldilocks-Quadratic on both M33 (+66 %) and Hazard3 (+15 %). The ISA-levelling result (cross-ISA 1.04×) is a legitimate separate finding, but end-to-end latency favors Goldilocks-Quadratic and that's the number a firmware integrator cares about.
