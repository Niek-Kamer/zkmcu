#import "/research/lib/template.typ": *

// Scoping spike for the PQ-Semaphore milestone (phase 4.0).
// Published before any AIR or verifier code lands, so the AIR-shape
// decisions and verify-cost predictions remain falsifiable. Same
// discipline as the BLS12-381 and STARK prediction reports.
//
// The audit milestone (zkmcu-poseidon-audit) closed 2026-04-28 and
// confirmed Plonky3's published Poseidon2-BabyBear (t=16, alpha=7,
// R_F=8, R_P=13) is the right hash to lock in for the AIR. This
// document scopes what an AIR using that hash looks like.

#show: paper.with(
  title: "PQ-Semaphore on RP2350: scoping spike + falsifiable predictions",
  authors: ("zkmcu",),
  date: "2026-04-29",
  kind: "report",
  abstract: [
    Scoping document for the post-quantum Semaphore milestone: replacing
    the Groth16 / BN254 verifier (currently 551 ms on Cortex-M33 per
    `2026-04-28-m33-bn254-rebench`) with a Plonky3 STARK over BabyBear
    using Poseidon2 as its Merkle hash. Locks the AIR shape, FRI
    parameters, and security target before any AIR code is written.
    Predicts on-MCU verify time *900--1800 ms* on Cortex-M33 and
    *1100--2300 ms* on Hazard3 RV32 at depth-10 Merkle membership +
    nullifier + scope binding, 95-bit conjectured security, with
    proof size *15--30 KB* and peak RAM *80--140 KB*. Confidence is
    moderate: the existing Plonky3 Fibonacci `N = 256` BabyBear bench
    (verify 29 ms, 11 queries, 21-bit security) is the load-bearing
    extrapolation point. Published before the `zkmcu-air-pq-semaphore`
    crate exists so the delta against measurement is interpretable as
    a scientific result.
  ],
)

= Why this document exists

The PQ roadmap locked on 2026-04-28 (after the Poseidon2 audit closed)
puts PQ-Semaphore as the next headline milestone: a STARK-based
Semaphore-equivalent verifier replacing the BN254 / Groth16 path.
Phase 2's BLS12-381 prediction report and phase 3.1's first STARK
prediction report both demonstrated that pre-build predictions are
load-bearing for interpreting later measurements as evidence rather
than narrative. This document is the equivalent for phase 4.0:
committed before the AIR exists, never edited after.

The audit milestone (`crates/zkmcu-poseidon-audit`, closed
2026-04-28) confirmed Plonky3's published `BabyBear` Poseidon2 instance
(`t = 16`, `α = 7`, `R_F = 8`, `R_P = 13`) is bit-identical to an
independent re-derivation of the round-count bounds and an independent
permutation port. That hash is therefore locked in for the AIR. The
remaining decisions are *what circuit* and *what parameters*.

= What "PQ-Semaphore" means in this scope

Semaphore v4 (the BN254 / Groth16 instance currently shipping in
`crates/zkmcu-vectors/data/semaphore-depth-10/`) proves four things
inside a single Groth16 proof:

+ The prover knows a secret identity commitment `id`.
+ `H(id)` is a leaf of a public Merkle tree at depth $D = 10$.
+ A nullifier `N = H(id, scope)` is correctly derived for a given
  scope, preventing double-signalling.
+ A signal hash binds the proof to a specific message, so it cannot be
  replayed against a different message.

PQ-Semaphore preserves the same four properties but recasts them as a
Plonky3 AIR over `BabyBear`, hashing with the audited Poseidon2 instead
of MiMC / Poseidon-128. Verifier-side it produces a STARK proof verified
by `winter-verifier` (or Plonky3's verifier — see § 3.1).

*Out of scope for this milestone.* No on-chain integration, no
trusted-setup-free aggregation, no recursion. Just on-MCU verify
of a single PQ-Semaphore proof for the phase-2 Pico 2 W deployment
target.

= Setup assumptions

== Target stack

- *Hardware.* Pi Pico 2 W (RP2350 Cortex-M33 @ 150 MHz and Hazard3 RV32
  @ 150 MHz; 520 KB SRAM). Same baseline as every prior phase.
- *Toolchain.* `rustc 1.94.1`, `release` profile with `lto = "fat"`,
  `opt-level = "s"`, `codegen-units = 1`, `panic = "abort"`. Same as
  phases 2 and 3.
- *Field.* `BabyBear` (`p = 15 dot.op 2^27 + 1 = 2,013,265,921`).
  31-bit prime, two-adicity 27. Locked by the hash choice — Poseidon2
  parameters were audited specifically for this field.
- *Hash.* Poseidon2 over `BabyBear`, `t = 16`, `α = 7`, `R_F = 8`,
  `R_P = 13`. Bit-identical to Plonky3's published instance, audited
  in `crates/zkmcu-poseidon-audit`. Sponge rate 2, capacity 14 (the
  capacity-14 split gives `14 dot.op 31 = 434` capacity bits, far
  above the 128-bit floor).
- *Verifier crate.* Open question — see § 3.1.
- *Security.* 95-bit conjectured. Same level as the existing BabyBear
  STARK benches (`2026-04-26-m33-stark-prover-bb`). Justified because
  conjectured-soundness STARKs at 95 bits sit comfortably above the
  policy floor (80 bits) used for production STARK deployments and
  match what the existing measured baselines already deliver, so the
  prediction is anchored to comparable measurements rather than a
  parameter set we have never benched.

== Open question 3.1: Plonky3 verifier vs winterfell

The phase-3.1 STARK prediction report committed to a winterfell
verifier (`zkmcu-verifier-stark`). That crate exists and ships a
Fibonacci AIR. But two phases since then have used the *Plonky3*
prover (`bench-rp2350-m33-stark-prover-bb` at phase 3.3) without a
matching Plonky3 verifier crate. The Poseidon2 we audited is
Plonky3's, not winterfell's.

Two paths:

+ *Stay on winterfell.* Implement the PQ-Semaphore AIR against
  `winter-air`. Reuse the existing `zkmcu-verifier-stark` plumbing.
  Cost: porting the Poseidon2-`BabyBear` hash from Plonky3 into
  winterfell's `Hasher` trait. winterfell's existing hashers are
  RP / Blake3, so this is non-trivial but bounded.

+ *Add a Plonky3 verifier crate.* Build `zkmcu-verifier-plonky3` as a
  new sibling. Reuse Plonky3's audited Poseidon2 as-is. Cost: a second
  STARK verifier in the firmware tree, each pulling its own dep
  closure.

This document predicts numbers under path 2 (Plonky3 verifier) because
the audited hash is Plonky3-native. If the spike lands on path 1
during build, the predictions still apply at the algorithm level
(same AIR, same security, same query count) and only the constant
factors shift.

== Measurement methodology

Identical to phases 2--3: `DWT::cycle_count` on Cortex-M33, `mcycle` on
Hazard3, $gt.eq$ 7 iterations per datapoint, USB-CDC serial output,
stack painting + `TrackingHeap` on the M33 firmware. SYS_HZ runtime
assertion (added 2026-04-28) guards against silent clock drift.

= AIR shape decisions

== Merkle tree depth

*Lock to D = 10.* Matches the existing Semaphore v4 deployment
(`crates/zkmcu-vectors/data/semaphore-depth-10/`), so the comparison
"same circuit, post-quantum" is direct. Depth-10 supports up to 1024
group members, which is the canonical Semaphore demo size.

== AIR layout

The AIR has three sub-circuits, all sharing trace columns:

+ *Merkle path* ($D = 10$ Poseidon2 invocations of `t = 2` permutation
  squeezed into the `t = 16` instance, one per level). Each Poseidon2
  call takes one trace row per round (R_F + R_P = 21 rows) plus
  state columns. Estimated trace cost: $10 dot.op 21 = 210$ rows
  per Merkle-path proof.
+ *Nullifier hash* `N = H(id, scope)`: one Poseidon2 invocation, 21
  rows.
+ *Scope binding* `S = H(scope, message)`: one Poseidon2 invocation,
  21 rows.

Total trace length budget: $approx.eq 252$ rows, rounded up to the next
power of two: $N = 256$ trace rows. Convenient because it matches the
existing `2026-04-26-m33-stark-prover-bb` Fibonacci `N = 256` baseline
exactly.

== Trace columns

State width 16 + auxiliary (round-constant selectors, conditional swap
booleans for the Merkle path direction bits): predict *24--32 trace
columns total*. Conservative upper bound 32 for sizing purposes.

== FRI parameters

Locked to match the existing BabyBear bench:

- Blowup factor: 4
- Folding factor: 4
- Max remainder degree: 7
- Number of queries: *64* (for 95-bit conjectured security with
  blowup 4, this is the canonical query count Plonky3's Fibonacci
  reference uses; the existing `2026-04-27-m33-stark-threshold-q64`
  run uses the same 64-query setting).
- Grinding bits: 0 (matches existing BabyBear benches).

LDE domain: $4 dot.op 256 = 1024$ rows.

= Predictions

== Verify time on Cortex-M33

Anchor point: `2026-04-26-m33-stark-prover-bb` Fibonacci `N = 256`,
2 columns, 11 queries, 21-bit security: verify ~29 ms.

PQ-Semaphore differs along three axes:

+ *Query count.* 11 → 64 queries. FRI verify cost is roughly linear
  in query count; a 64-query proof costs ~5.8× more in FRI checks
  than an 11-query proof.
+ *Trace columns.* 2 → ~32 columns. Per-query Merkle authentication
  paths into the trace commitment grow with the number of committed
  columns, but only the path *length* (= log2 of LDE size) is fixed
  at 10. The cost of opening more columns at each query is ~16×
  (additional Poseidon2 hashes batched at each layer). Bounded by
  layer-per-layer Poseidon2 batching factor; effective cost multiplier
  ~3--5× rather than 16×.
+ *Constraint evaluation at OOD.* Poseidon2 `α = 7` constraints have
  degree 7. Existing benches use Fibonacci's degree-2 constraints. OOD
  evaluation cost scales with constraint complexity, predicted +20--40 %
  on top of base verify.

#compare-table(
  ("Component", "Anchor (Fibonacci N=256, 11 queries)", "PQ-Semaphore scaling", "Predicted"),
  (
    ([Base verify (FRI + Merkle)], [~25 ms], [×5.8 (queries) × ~1.5 (columns)], [~220 ms]),
    ([OOD constraint evaluation], [~3 ms], [×6 (deg 7 vs deg 2)], [~18 ms]),
    ([Public-input parsing], [~1 ms], [unchanged], [~1 ms]),
    ([Poseidon2 hash batching for FRI], [included above], [×3--5 vs Fibonacci], [implicit in row 1]),
  ),
)

Point estimate: *~240 ms*. Wide interval to account for the column-count
multiplier being a guess: *plausible range 150--400 ms*.

But the existing 64-query threshold bench
(`2026-04-27-m33-stark-threshold-q64`) lands at a higher number than
the 11-query case scaled linearly, suggesting the per-query overhead
is heavier on Plonky3 than the Fibonacci baseline implies. Adjusting
upward for this:

*Final prediction: 900--1800 ms verify on Cortex-M33.* Wide because
the column-count and Poseidon2-batching multipliers compound. Point
estimate *1200 ms*. Higher than the 551 ms BN254 baseline — the
"PQ" gain costs ~2--3× verify time. That is the headline trade.

== Verify time on Hazard3 RV32

Phase 3.3 measured the BabyBear M33/RV32 ratio at 1.04× (tighter than
any earlier phase: 2026-04-24 BabyBear×Quartic). Apply the same ratio:

*Prediction: 940--1880 ms on Hazard3 RV32, point estimate 1250 ms.*
RV32 within 5 % of M33, just like phase 3.3.

== Proof size

64 queries × FRI layers (`log_4(1024) = 5` FRI layers + final remainder)
× Merkle authentication paths × ~32 trace columns. Existing 11-query
threshold bench: 5,787 bytes. Scale linearly with queries (×5.8) and
sublinearly with columns (Merkle paths share ancestors): *~30 KB*.

#compare-table(
  ("Quantity", "Predicted value"),
  (
    ([Proof size], [15--30 KB]),
    ([Verify working heap], [60--100 KB]),
    ([Peak stack during verify], [4--8 KB]),
    ([Total RAM during verify], [80--140 KB]),
  ),
)

*Tier implications.* 80--140 KB total RAM fits the 256 KB SRAM tier
comfortably. Borderline on the 128 KB tier, depending on heap fragmentation.

== Prove time on Cortex-M33 (informational, not the milestone)

The phase 3.3 BabyBear prove benches lands `N = 256` Fibonacci prove at
~148 ms. PQ-Semaphore at the same `N = 256` with more columns and more
queries: *prove ~600--1200 ms M33*. Mentioned for completeness; the
milestone is verify-only firmware. Proving stays on the host.

== Variance

Predict *0.05--0.15 % iteration-to-iteration variance.* Slightly wider
than the 0.030 % Semaphore baseline because the larger working heap
and larger code footprint of a STARK verifier introduces more
opportunities for cache-line jitter at the bench level. Still tighter
than 0.15 % is the falsifiable bar.

= Falsification criteria

The following outcomes would invalidate the predictions above and
warrant a separate published explanation:

- *Verify on M33 outside 500 ms -- 2500 ms.* Below 500 ms means the
  per-query Poseidon2 hashing cost is dramatically lower than the
  existing threshold-q64 datapoint suggests, which would be a separate
  finding. Above 2500 ms means the column-count multiplier is worse
  than predicted, or the AIR ended up wider than the 32-column upper
  bound.
- *Verify exceeds the BN254 baseline by more than 4×.* Headline target
  is 2--3× slower than the 551 ms Groth16 baseline. > 4× means
  PQ-Semaphore on this hardware is impractical for the deployment use
  cases (hardware wallets, offline gates) that motivate the milestone.
- *Proof size outside 8--60 KB.* Below 8 KB means FRI parameters are
  more compact than scaling implies. Above 60 KB pushes against the
  practical wire-transport budget for Bluetooth / NFC delivery to
  the MCU, which is part of the deployment narrative.
- *Peak heap exceeds 200 KB.* Disqualifies the 256 KB tier with
  comfortable margin and forces either streaming verify or aggressive
  parameter tuning. Publishable either way.
- *RV32 / M33 verify ratio outside 0.85--1.20×.* Phase 3.3 measured
  1.04× on BabyBear+Quartic. Significant divergence here would mean
  the AIR's column structure exposes a different ISA-balance pattern
  than Fibonacci did.

= Phase 4.0 deliverables (in build order)

+ *This document.*
+ Resolution of § 3.1: choose Plonky3 verifier vs winterfell port.
  Spike both options for ~1 day each, decide based on dep-closure
  size + Poseidon2 hash availability.
+ `crates/zkmcu-air-pq-semaphore`: AIR definition (Merkle path +
  nullifier + scope binding) for the chosen verifier framework. Pure
  trace-and-constraint logic, no I/O. `no_std` from day one.
+ Host-side prover wrapper: a thin CLI in `zkmcu-host-gen` that takes
  an identity seed + scope + message + Merkle witness and emits
  `proof.bin` + `public.bin`. Mirror of the existing semaphore
  generator pattern (`scripts/gen-semaphore-proof/`).
+ Host-side cross-check test: re-verify the bytes using the
  framework's own verifier before they hit `crates/zkmcu-vectors/`.
  Same discipline as every prior phase.
+ `bench-rp2350-m33-pq-semaphore` + `bench-rp2350-rv32-pq-semaphore`
  firmware crates. Minimum change from the existing BabyBear stark
  template: swap AIR, swap vector, time parse + verify as a single
  block.
+ First measurement runs under
  `benchmarks/runs/<date>-{m33,rv32}-pq-semaphore-d10/`.
+ Comparison report
  `research/reports/<date>-pq-semaphore-results.typ` quoting this
  document's predictions verbatim and stating measured values. This
  scoping document does not get edited after commit.

= Explicit non-claims

- *Not a claim that PQ-Semaphore on MCU is faster than BN254
  Semaphore on MCU.* The point estimate is 2--3× slower at 95-bit
  STARK security vs ~128-bit pairing security. The headline is
  *post-quantum on MCU at all*, not *post-quantum faster*.
- *Not a claim of full Semaphore v4 wire compatibility.* The
  protocol-level guarantees (membership, nullifier uniqueness, scope
  binding) are preserved but the proof bytes do not interoperate with
  the Ethereum Semaphore precompile. A separate aggregation /
  recursion phase would be needed for that, out of scope here.
- *Not a claim that this is the optimal AIR shape.* Width 32 columns
  is an upper bound, not the result of a custom column-packing pass.
  Phase 4.x territory.
- *Not a claim that BabyBear is the only viable PQ field for this
  workload.* Mersenne-31 or KoalaBear could plausibly outperform
  BabyBear on the same MCU. Comparison is phase 4.x.
- *Not a claim that 95-bit conjectured security is the right
  parameter for production deployment.* Production should target
  $gt.eq$ 128 bits provable. The 95-bit choice is for comparability
  with existing benches; a 128-bit version is a parameter sweep
  away once the v1 number is measured.

= Open questions to resolve during phase 4.0

- *Plonky3 verifier vs winterfell port.* See § 3.1. Resolve in week 1.
- *Public-input encoding.* Semaphore v4 has 4 public inputs (root,
  nullifier, signal hash, scope). PQ-Semaphore needs the same, but
  encoded as `BabyBear` field elements. Each public input is one
  `BabyBear` element + a hash digest, so ~5--8 field elements total.
  Confirm during AIR write-up.
- *Conditional-swap encoding for Merkle path direction bits.* The 10
  direction bits per path can be either witness columns (cheap trace,
  expensive constraint) or unrolled into the column structure
  (expensive trace, cheap constraint). Decide during AIR design.
- *Vector commit hygiene.* Test vectors at depth 10 should be
  byte-deterministic under a fixed seed (matching the phase-2 v4
  Semaphore generator pattern). Confirm regen-vectors workflow
  covers the new AIR.

= References

The audit milestone (`crates/zkmcu-poseidon-audit`) confirms Plonky3's
Poseidon2-`BabyBear` parameters match an independent re-derivation:
`tests/perm_diff.rs` at HEAD passes 10 cases (including 200 random
seeds). Citation for the hash itself: Grassi, Khovratovich, Schofnegger,
*Poseidon2: A Faster Version of the Poseidon Hash Function* (IACR
ePrint 2023/323, https://eprint.iacr.org/2023/323).

The phase-2 Semaphore baseline: `research/reports/2026-04-22-semaphore-baseline.typ`
and `crates/zkmcu-vectors/data/semaphore-depth-10/`.

The anchor measurements for STARK extrapolation:
`benchmarks/runs/2026-04-26-m33-stark-prover-bb/result.toml` (Fibonacci,
N=256, 11 queries, 21-bit, verify 29 ms) and
`benchmarks/runs/2026-04-27-m33-stark-prover-threshold/result.toml`
(threshold-check, N=64, 11 queries, verify 29 ms — confirms FRI query
count rather than trace size dominates verify cost).

The Groth16 / BN254 baseline this milestone replaces:
`benchmarks/runs/2026-04-28-m33-bn254-rebench/result.toml` (Groth16
verify 551 ms M33, 808 ms RV32).
