#import "/research/lib/template.typ": *

// Phase 4.0 headline results report. Scores the predictions made in
// 2026-04-29-pq-semaphore-scoping.typ against the measurements taken
// the same day. Immutable after publication: any later re-measurement
// gets a new dated report.

#let m33  = toml("/benchmarks/runs/2026-04-29-m33-pq-semaphore/result.toml")
#let rv32 = toml("/benchmarks/runs/2026-04-29-rv32-pq-semaphore/result.toml")
#let chain_m33  = toml("/benchmarks/runs/2026-04-29-m33-pq-poseidon-chain/result.toml")
#let chain_rv32 = toml("/benchmarks/runs/2026-04-29-rv32-pq-poseidon-chain/result.toml")

#show: paper.with(
  title: "PQ-Semaphore on RP2350: 1.05 s M33 / 1.25 s RV32, prediction lands on point on Hazard3",
  authors: ("zkmcu",),
  date: "2026-04-29",
  kind: "report",
  abstract: [
    Phase 4.0 headline result. The depth-10 PQ-Semaphore STARK verifier
    runs in *1049.72 ms* on Cortex-M33 and *1249.59 ms* on Hazard3 RV32,
    both inside the published 900--1800 ms / 940--1880 ms scoping band.
    The RV32 number lands within 0.08 % of the published 1250 ms point
    estimate. Variance is *0.029 % M33 / 0.055 % RV32* across 19 + 21
    iterations, the tightest M33 measurement in the project to date.
    Cross-ISA gap *1.190×*, the smallest non-trivial cross-ISA ratio
    measured on this hardware. Vs the Groth16 / BN254 baseline this
    milestone replaces: M33 pays a 1.91× cost (PQ tax in the predicted
    range), RV32 is 9 % *faster* than Groth16 (the PQ tax inverts on
    the ISA where pairing-based crypto is under-tuned). Proof is
    169 KB, 5.5× over the predicted 30 KB ceiling, driven by the
    64-query FRI parameterisation needed for 95-bit conjectured
    security. Heap is well within the 384 KB budget. The headline
    target is met; the deployment cost is in proof size, not verify
    time or memory.
  ],
)

= What we predicted

Quoted verbatim from `2026-04-29-pq-semaphore-scoping.typ` § 5:

#quote(block: true)[
  *On-MCU verify time:* 900--1800 ms on Cortex-M33, point estimate
  1200 ms; 940--1880 ms on Hazard3 RV32, point estimate 1250 ms.
  *Proof size:* 15--30 KB. *Peak RAM:* 80--140 KB. *Variance:*
  0.05 -- 0.15 %. *Stack peak:* 4--8 KB.
]

The scoping doc was published before any AIR code existed. The
predictions were anchored on the existing Plonky3 Fibonacci `N=256`
BabyBear bench (29 ms verify at 11 queries) plus an extrapolation
factor for query count, trace columns, and constraint complexity.
Same-day chain anchor (`2026-04-29-{m33,rv32}-pq-poseidon-chain`)
suggested the band might be too pessimistic; the headline numbers
clarify that the 64-query parameterisation pulls the cost back into
the published range.

= What we measured

== Cortex-M33

#compare-table(
  ("Quantity", "Predicted band", "Predicted point", "Measured", "Verdict"),
  (
    ([Verify (ms)],          [900--1800],  [1200],  [#m33.bench.pq_semaphore_verify.us_median / 1e3],     [inside, lower-middle]),
    ([Proof size (KB)],      [15--30],     [30],    [#calc.round((m33.circuit.proof_bytes / 1024) * 100) / 100], [#text(red)[over 5.5×]]),
    ([Heap after parse (KB)],[80--140],    [110],   [#calc.round((m33.footprint.heap_after_parse / 1024) * 100) / 100],     [#text(orange)[over ~7 %]]),
    ([Variance (%)],         [0.05--0.15], [0.10],  [#m33.bench.pq_semaphore_verify.range_pct], [#text(rgb("#0a7d3a"))[below band, tightest in project]]),
    ([Stack peak (KB)],      [4--8],       [6],     [n/a],   [not captured this run]),
  ),
)

== Hazard3 RV32

#compare-table(
  ("Quantity", "Predicted band", "Predicted point", "Measured", "Verdict"),
  (
    ([Verify (ms)],          [940--1880],  [1250],  [#rv32.bench.pq_semaphore_verify.us_median / 1e3], [#text(rgb("#0a7d3a"))[on point, 0.08 % off]]),
    ([Proof size (KB)],      [15--30],     [30],    [#calc.round((rv32.circuit.proof_bytes / 1024) * 100) / 100], [#text(red)[over 5.5×]]),
    ([Heap after parse (KB)],[80--140],    [110],   [#calc.round((rv32.footprint.heap_after_parse / 1024) * 100) / 100], [#text(orange)[over ~7 %]]),
    ([Variance (%)],         [0.05--0.15], [0.10],  [#rv32.bench.pq_semaphore_verify.range_pct], [inside, lower edge]),
    ([Stack peak (KB)],      [4--8],       [6],     [n/a],   [not captured this run]),
  ),
)

The verify time on RV32 lands within rounding of the published point
estimate. This is the cleanest match between a published prediction
and a measurement in the project to date — and partly lucky. The
scoping doc derived the RV32 number by applying a 1.04× cross-ISA
factor (from phase 3.3 BabyBear × Quartic) to the M33 prediction.
That factor is ~14 % off the actual 1.19× ratio. Both the M33
prediction overshooting and the cross-ISA factor undershooting
cancel, and the RV32 number lands on point.

= Vs Groth16 / BN254 — the headline framing

The scoping doc § 5 framed the cost: *"the headline target is 2--3×
slower than the 551 ms Groth16 baseline. > 4× would be impractical
for the embedded use case."* Measured:

#compare-table(
  ("ISA", "Groth16/BN254 verify (ms)", "PQ-Semaphore verify (ms)", "PQ tax"),
  (
    ([Cortex-M33], [550.67], [#m33.bench.pq_semaphore_verify.us_median / 1e3],  [*1.91×*]),
    ([Hazard3 RV32], [1363.23], [#rv32.bench.pq_semaphore_verify.us_median / 1e3], [*0.92× (faster)*]),
  ),
)

M33 sits at the lower end of the predicted 2--3× tax. RV32 inverts
the framing entirely: PQ-Semaphore is 9 % faster than the BN254 verify
on the same silicon, because Hazard3 lacks the M33's UMAAL-accelerated
big-integer multiply that the BN254 `U256::mul` asm depends on.
Pairing-based crypto on RV32 is under-tuned in a way that PQ is not.

This is a substantive shift from the scoping framing. The published
abstract said "the PQ tax is real on this hardware". On Cortex-M33
that holds. On Hazard3 RV32 it doesn't — the PQ path is the cheaper
option, full stop. For deployments that target both ISAs (the same
silicon, different cores) a single STARK verifier path is now the
better choice on portability grounds even before the PQ-security
argument.

= Cross-ISA: the gap closes

#compare-table(
  ("Workload", "M33 (ms)", "RV32 (ms)", "Ratio"),
  (
    ([Groth16 / BN254 verify],         [550.67],  [1363.23], [2.20×]),
    ([Plonky3 chain anchor (28 q)],    [#chain_m33.bench.pq_poseidon_chain_verify.us_median / 1e3],  [#chain_rv32.bench.pq_poseidon_chain_verify.us_median / 1e3], [1.252×]),
    ([*PQ-Semaphore (64 q)*],        [#m33.bench.pq_semaphore_verify.us_median / 1e3], [#rv32.bench.pq_semaphore_verify.us_median / 1e3], [*1.190×*]),
  ),
)

The 1.19× cross-ISA ratio is the smallest non-trivial gap measured
on this hardware. Tighter than the chain anchor (1.252×) — the
heavier constraint evaluation in the headline AIR (Merkle hops + scope
binding + nullifier) is plain BabyBear arithmetic which Hazard3
handles essentially as well as M33; the chain anchor is more
Poseidon2-bound, where M33's slightly better cache and pipeline
behaviour shows up more.

The deeper point: STARK verify on a 31-bit field is *cross-ISA
portable* in a way pairing-based verify is not. The BN254 verify
ratio of 2.20× was real engineering work (UMAAL hand-asm for M33;
no equivalent on RV32). The PQ verify ratio of 1.19× falls out of
the field choice for free.

= Where the prediction was wrong: proof size

5.5× over the upper bound. The scoping doc wrote 15--30 KB based on
"Plonky3 proofs at log_blowup=1, ~24 queries, narrow trace". The
actual config uses 64 queries to hit 95-bit conjectured security.
Per-query Merkle openings dominate proof size linearly:

- 28 queries → 90 KB (chain anchor)
- 64 queries → 169 KB (this measurement)

Ratio 1.87× for 2.29× the queries, slightly sub-linear because some
proof bytes are query-count-independent (commitments, public inputs,
constraint ZH evaluations). The scoping doc anchored on a
query-count assumption that didn't reflect the security floor for
the deployment.

The proof fits in flash on the 4 MB Pico 2 W with margin, so this
is not a deployment blocker. But it changes the framing of "compact
proofs" — PQ-Semaphore proofs are 660× the size of Groth16 / BN254
proofs (256 B vs 169 KB). For on-chain or bandwidth-constrained
applications that ratio matters; for on-MCU verify it doesn't.

= Variance discipline

The 0.029 % M33 variance is the tightest measurement in the project.
Prior leader was the Phase 3.3 BabyBear × Quartic STARK verify at
0.053 %. This run hit 19 iterations with cycle counts ranging
#m33.bench.pq_semaphore_verify.cycles_min to
#m33.bench.pq_semaphore_verify.cycles_max — a spread of #(m33.bench.pq_semaphore_verify.cycles_max - m33.bench.pq_semaphore_verify.cycles_min) cycles
out of ~157.4 M.

That this holds on the heaviest workload measured to date (321 trace
columns, 64 queries, ~280 KB heap working set) is a real result. The
combination of `bench-core::measure_cycles` discipline + TLSF
allocator + fully deterministic Plonky3 verify path is doing real
work suppressing noise. Run-to-run noise is at the level of DWT
cycle-counter granularity, not at the level of allocator
fragmentation or USB interrupt jitter.

RV32 is consistently noisier (0.055 % here, 0.076 % on the chain
anchor, 0.078 % on phase 3.3). No theory yet; possibly cache-line
behaviour around the global allocator, or interrupt jitter from
mcycle reads vs M33's DWT path.

= What got falsified, what didn't

*Verify time* (M33 + RV32): inside band, RV32 on point. Held.

*Variance*: predicted 0.05 -- 0.15 %, measured *below* the band on
M33 (0.029 %), at the lower edge on RV32 (0.055 %). Both held in the
sense of "this is a tight measurement"; the M33 number falsifies the
*lower bound* of the prediction, in the user-friendly direction.

*Proof size*: falsified high, by 5.5×. The scoping doc's query-count
assumption was wrong. Proof size scales linearly with queries; the
deployment-relevant security floor of 95 bits requires 64 queries at
log_blowup=1, not the ~24 the scoping doc imagined.

*Heap*: heap_after_parse at 150 KB exceeded the 80 -- 140 KB upper
bound by ~7 %. Verify-time peak was not captured this run (USB
write_line dropped during the heavy section that doesn't poll the
bus); estimated 250 -- 280 KB based on the chain anchor's 216 KB at
28 queries. Either way well within the 384 KB heap budget.

*Stack peak*: not captured. Chain anchor was 2.4 KB; expected small
bump for the headline AIR's heavier constraint-eval recursion, no
reason to think it crosses 8 KB.

= What this means for phase 4.x

The PQ tax is met on M33 (1.91×) and inverted on RV32 (0.92×). The
scoping doc's *2--3× tax, > 4× impractical* framing is now
obsolete: on RV32 the PQ path is unambiguously cheaper. A unified
STARK-only verifier across both cores is now a defensible
architectural choice, not just a security one.

Two forward-looking measurements would close this milestone cleanly:

+ A re-flash with USB-poll-aware fence posts (poll the bus inside
  `make_config` and `build_air`, or pace 50 ms between fence posts),
  capturing the `[boot]` line for `stack_peak` and full `heap_peak`.
  Pure metric-completeness work; doesn't change the headline.
+ A 28-query variant of the headline AIR for an apples-to-apples
  comparison with the chain anchor at fixed query count. Would
  isolate the constraint-complexity cost from the query-count cost
  in the chain → headline jump (currently entangled at 2.13×
  slowdown).

Neither is on the milestone-close critical path. Phase 4.0
substantively succeeded.

= Reproducibility

Bench artifacts:
- `benchmarks/runs/2026-04-29-m33-pq-semaphore/`
- `benchmarks/runs/2026-04-29-rv32-pq-semaphore/`

Vectors (committed, byte-deterministic, regenerable via
`just regen-vectors`):
- `crates/zkmcu-vectors/data/pq-semaphore-d10/proof.bin` (168 970 B)
- `crates/zkmcu-vectors/data/pq-semaphore-d10/public.bin` (64 B)

Verifier crate: `crates/zkmcu-verifier-plonky3/src/pq_semaphore.rs`.
Firmware: `crates/bench-rp2350-{m33,rv32}-pq-semaphore`.
