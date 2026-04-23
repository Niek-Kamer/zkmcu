#import "/research/lib/template.typ": *

// Prediction report for phase 3.2: re-benching the STARK Fibonacci verifier
// under FieldExtension::Quadratic (production-grade ~96-bit security).
// Committed before the prover is switched over and before any firmware is
// re-flashed, so the predictions remain falsifiable. Same discipline as
// 2026-04-22-bls12-381-prediction.typ and 2026-04-23-stark-prediction.typ.

#show: paper.with(
  title: "STARK verify at quadratic extension: predicted performance (pre-measurement)",
  authors: ("zkmcu",),
  date: "2026-04-24",
  kind: "report",
  abstract: [
    Phase 3.1 measured STARK Fibonacci verify on RP2350 at 63-bit conjectured
    security (Goldilocks, no extension). That number is below production use.
    Phase 3.2 switches the prover to `FieldExtension::Quadratic` to lift
    security to ~96-bit, re-benches on the same silicon, and asks whether
    the 128 KB SRAM tier finding from phase 3.1 still holds. This document
    freezes numerical predictions *before* any code is changed: M33 verify
    *90--140 ms*, RV32 verify *130--210 ms*, proof size *40--50 KB*, total
    RAM *100--150 KB*. Most consequential falsifiable claim: *total RAM
    stays under 128 KB*, i.e. the "all three verifier families on one
    hardware-wallet-tier SRAM budget" narrative survives phase 3.2.
    Confidence is moderate --- phase 3.1 measurements anchor the starting
    point, but the quadratic-extension cost model is extrapolated from
    winterfell internals, not measured directly.
  ],
)

= Why this document exists

Phase 3.1 published a headline: *Cortex-M33 STARK Fibonacci verify in
43.8 ms at 76 KB total RAM*, comfortably under the 128 KB hardware-
wallet SRAM tier. That measurement was taken at 63-bit conjectured
security with `FieldExtension::None` --- the cheapest configuration
winterfell will accept.

63-bit is below any plausible production threshold. The grant-pitch
line "all three verifier families on 128 KB SRAM" is only defensible
if the STARK number survives being hardened to a production-grade
security level. Phase 3.2 does that: prover switches to
`FieldExtension::Quadratic`, which lifts Goldilocks-based conjectured
security to ~96 bits, matching winterfell's own Fibonacci reference
configuration.

This document goes into git *before* the prover is changed. A later
`2026-04-24-stark-quadratic-results.typ` will quote these numbers
verbatim next to the measured values.

= Baseline: what phase 3.1 measured

Anchoring all predictions to phase 3.1's numbers rather than
re-deriving from cycle-count first principles:

#compare-table(
  ("Quantity", "Phase 3.1 measured", "Config"),
  (
    ([M33 verify time], [43.8 ms], [Goldilocks, no ext., 32 queries]),
    ([RV32 verify time], [64.1 ms], [same]),
    ([Proof size], [25,332 B], [same]),
    ([M33 heap peak], [70.7 KB], [same]),
    ([M33 stack peak], [5.2 KB], [same]),
    ([M33 total RAM], [~76 KB], [heap + stack + statics]),
    ([Conjectured security], [63 bits], [below production]),
  ),
)

= What changes with `FieldExtension::Quadratic`

Under quadratic extension, FRI / DEEP composition / constraint
combining all move from $F_p$ (64-bit Goldilocks) to $F_(p^2)$
(a degree-2 extension). The trace itself stays in the base field.

Concrete cost deltas, per-component:

- *Trace Merkle commitment.* Leaves stay base-field. Hash work
  identical to phase 3.1.
- *FRI layer commitments.* Layer evaluations are in $F_(p^2)$ --- each
  evaluation is 16 B instead of 8 B. With folding factor 8, a leaf
  is now 128 B (2 Blake3 compressions) instead of 64 B (1 compression).
  Leaf hashing doubles. Auth-path internal nodes are still 2×32 B =
  1 compression, unchanged.
- *Field arithmetic in verify.* Each $F_(p^2)$ mul is ≈ 3 base muls +
  1 reduction (Karatsuba). DEEP consistency checks and FRI fold
  consistency checks all pay this ~3× cost. Constraint OOD evaluation
  also moves to $F_(p^2)$.
- *OOD evaluations serialised in proof.* Each OOD eval is now 16 B
  instead of 8 B. Small absolute bytes.
- *Query count.* Stays at 32 under the same `ProofOptions`.
- *Trace width, AIR shape, assertion count.* Unchanged.
- *Hash function.* Unchanged (Blake3-256).

= Predictions

== Verify time on Cortex-M33

Decomposing phase 3.1's 43.8 ms by rough component guess:

- Blake3 hash work: ~55 % ≈ 24 ms
- Goldilocks field arithmetic: ~30 % ≈ 13 ms
- Parser + scratch allocator work: ~15 % ≈ 7 ms

Scaled to quadratic extension:

- Hash work grows ~30 %: *24 → 31 ms* (leaf-level doubling, auth paths unchanged).
- Field arithmetic grows ~3×: *13 → 40 ms* (most verify-side ops move to $F_(p^2)$).
- Parser + scratch grows modestly since the proof is larger: *7 → 11 ms*.

Sum: *~80 ms* point estimate.

Uncertainty: the 3× multiplier on field arithmetic assumes Karatsuba
and a cold reduction path; if winterfell's `F_(p^2)` implementation is
schoolbook (4 muls + reduction) or if reduction dominates, the
multiplier could be ~4×, pushing field arithmetic to ~50 ms and total
to ~90 ms. Going the other direction, if hash work is actually a
larger fraction than 55 %, the total could stay closer to 70 ms.

Final prediction: *90--140 ms on Cortex-M33*. Point estimate 110 ms.

== Verify time on Hazard3 RV32

Phase 3.1 measured RV32 at 1.46× M33 (64.1 ms / 43.8 ms). Both
sub-workloads that make up verify (hash + field arithmetic) scale
similarly across the two ISAs --- Blake3 is ROT-heavy (RV32
without Zbkb fallback-path) and Goldilocks mul depends on
`mul`+`mulhu` vs `UMULL` which was the original M33-wins pattern.
Quadratic extension amplifies the field-arithmetic portion, which is
the M33-favouring component.

Predicted ratio: *1.45--1.65×*, slightly wider than phase 3.1's
measured 1.46× because the extension-field work is more M33-favourable
than base-field work.

Final prediction: *130--210 ms on Hazard3 RV32*. Point estimate 170 ms.

== Proof size

Phase 3.1 proof: 25,332 B at `FieldExtension::None`. Breakdown
(rough, from winterfell serialisation shape):

- Trace Merkle auth paths (32 queries × ~13-layer depth × 32 B/hash)
  ≈ 13 KB. Unchanged.
- FRI layer commitments + auth paths ≈ 8 KB. Auth paths unchanged.
- FRI layer evaluations (32 queries × 13 layers × folding 8 × 8 B)
  ≈ 3 KB. *Doubles to ~6 KB* under quadratic extension.
- OOD evaluations + constraint evaluations ≈ 1 KB. Roughly doubles.
- Bookkeeping / headers ≈ 0.3 KB. Unchanged.

Predicted total: *40--50 KB*. Point estimate 44 KB.

== Heap peak

Phase 3.1: 70.7 KB, of which 50.7 KB is the parsed-proof resident set
and 20 KB is verify-time scratch. Parsed-proof size scales with
serialised proof size; scratch scales with query count × layer count
(unchanged) but with bigger field elements in the FRI state.

Parsed proof: *50.7 → ~85 KB* (factor 1.7, matching proof-size scaling).
Verify scratch: *20 → ~30 KB* (extension-field state, ~1.5×).

Predicted heap peak: *110--140 KB*. Point estimate 120 KB.

== Stack peak

Phase 3.1: 5.2 KB. winterfell's verifier mostly routes state through
the heap allocator rather than stack frames; extension-field types are
`Copy` 16-byte values rather than `Box`-ed, so stack use should barely
change.

Predicted stack peak: *5.2--7 KB*. Point estimate 5.5 KB.

== Total RAM

Heap peak + stack peak + statics (~500 B).

Predicted total: *115--150 KB*.

This is *the* critical prediction. Phase 3.1's headline was "all three
verifier families fit on 128 KB SRAM". If total RAM at
`FieldExtension::Quadratic` comes in at or under 128 KB, the headline
holds at production security. If it comes in over 128 KB, the
narrative shifts to "BN254 + BLS12 Groth16 on 128 KB, production
STARK on 256 KB".

Point estimate 126 KB --- right at the boundary. This is where the
measurement matters most.

== Variance

Phase 3.1 measured 0.30 % on M33, 0.69 % on RV32. The
`proof.clone()`-inside-timed-window hypothesis from phase 3.1 applies
equally here --- bigger proof means bigger clone allocation means
potentially more jitter. Slight upward bias expected but same order
of magnitude.

Predicted variance: *0.3--0.8 % on M33, 0.5--1.2 % on RV32*.

= Falsification criteria

- *M33 verify time outside 90--140 ms.* Below 90 ms means the
  $F_(p^2)$ cost multiplier is smaller than 3× --- winterfell is
  cheaper on extension-field arithmetic than the model assumes.
  Above 140 ms means the multiplier is closer to 4--5× or hash
  work grew more than the 30 % estimate.
- *Proof size outside 40--50 KB.* Would falsify the FRI-layer-eval
  scaling model.
- *Total RAM above 140 KB.* Falsifies the "STARK still fits 128 KB"
  headline. Would be the most consequential miss direction because
  it changes the grant pitch.
- *Cross-ISA ratio outside 1.3--1.7×.* Would suggest the M33-vs-RV32
  pattern isn't just "more field arithmetic amplifies M33 advantage"
  --- maybe Zbkb-free RV32 suffers on extension-field reduction in
  ways the model doesn't capture.
- *Variance above 1.5 % on either target.* Would suggest something
  structural about the verify path, not just the `proof.clone()`
  allocator jitter hypothesis.

= Phase 3.2 deliverables (in build order)

+ *This document.*
+ Switch `zkmcu-host-gen::stark::run` to construct `ProofOptions`
  with `FieldExtension::Quadratic`.
+ Raise `MinConjecturedSecurity` in `zkmcu-verifier-stark::fibonacci::verify`
  from 63 to 95 (matches the production-grade configuration).
+ Regen `crates/zkmcu-vectors/data/stark-fib-1024/proof.bin`. The
  host-side self-verify already runs during generation --- if the
  verifier's `MinConjecturedSecurity(95)` rejects the proof, the new
  `ProofOptions` do not meet 95-bit security and the prover config needs
  further tuning (e.g. more queries or higher grinding).
+ Rebuild both firmware crates. No firmware-code change needed --- they
  consume the committed proof bytes.
+ Re-flash M33, capture ≥ 16 iterations. Ingest into
  `benchmarks/runs/2026-04-24-m33-stark-fib-1024-q/`.
+ Re-flash RV32, capture ≥ 16 iterations. Ingest into
  `benchmarks/runs/2026-04-24-rv32-stark-fib-1024-q/`.
+ Results report `research/reports/2026-04-24-stark-quadratic-results.typ`
  quoting this document's predictions and stating measured values.

= Explicit non-claims

- *Not a claim that `FieldExtension::Quadratic` is the right security
  configuration for every deployment.* For audit-grade security,
  $F_(p^3)$ (cubic) or switching to a larger prime (e.g. Mersenne-31
  variant) may be preferred. The quadratic-extension number is a
  middle datapoint, not a recommendation.
- *Not a claim that 96-bit conjectured security equals 96-bit provable
  security.* Winterfell's `MinConjecturedSecurity` uses the conjectured
  list-decoding bound, which is the working assumption of most
  deployed STARK systems but is not proven. Provable security is
  lower by ~2× queries.
- *Not a claim that switching to quadratic extension is the only
  phase-3.2-era knob.* Raising grinding from 0 to (say) 20 would add
  another ~20 bits of security with negligible verify-time cost. That
  is a separate phase-3.2 sub-experiment that does not interact with
  this one.

= Open questions to resolve during phase 3.2

- *Does `MinConjecturedSecurity(95)` accept the quadratic-extension
  proof at 32 queries + blowup 8 + grinding 0?* If it rejects at 95
  but accepts at (say) 90, the actual security level of winterfell's
  default Fibonacci-style config is more nuanced than "switch
  extension = 96 bits". Worth documenting whatever number winterfell
  settles on.
- *Is the `proof.clone()` inside the timed window causing the variance
  anomaly?* The bigger proof in phase 3.2 makes the clone more expensive,
  so if variance scales with proof size, it points at the clone.
  Phase-3.2 follow-up is to move the clone outside the timed window
  and re-bench --- separate from this prediction.

#v(1em)

_Phase 3.1 results report_: `research/reports/2026-04-23-stark-results.typ`.

_Phase 3.1 prediction report_: `research/reports/2026-04-23-stark-prediction.typ`.
