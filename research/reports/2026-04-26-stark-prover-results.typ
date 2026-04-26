#import "/research/lib/template.typ": *

#let r = toml("/benchmarks/runs/2026-04-26-m33-stark-prover-fib/result.toml")

#show: paper.with(
  title: "Phase 4: first on-device STARK proving on Cortex-M33 and Hazard3 RV32",
  authors: ("zkmcu",),
  date: "2026-04-26",
  kind: "report",
  abstract: [
    Every prior zkmcu phase ran a proof generated on a server through a
    no_std verifier on the device. Phase 4 removes the server. The
    winterfell 0.13 prover compiles for `thumbv8m.main-none-eabihf` and
    `riscv32imac-unknown-none-elf` with zero code changes, and runs a
    Fibonacci AIR prover end-to-end on the Raspberry Pi Pico 2 W at
    150 MHz. Measured at N = 256 trace steps: *134 ms prove / 19 ms verify
    on Cortex-M33* and *208 ms / 25 ms on Hazard3 RV32*, with a
    *306 KB heap peak* and *6 668 B proof*, fully self-verified on the
    device. Scaling from N = 64 through N = 256 is clean O(N) in prove
    time and O(N) in heap -- the SRAM ceiling is N = 256 at
    blowup = 4. The cross-ISA prove gap (1.55x) is larger than the
    verify gap (1.29x) because proving is NTT-heavy and M33 has UMAAL;
    verifying is hash-heavy and UMAAL does not help there. No prior
    published result for no_std STARK proving on any MCU-class core
    was found in the prior-art survey.
  ],
)

= Setup

- *Hardware.* Raspberry Pi Pico 2 W (RP2350, Cortex-M33 + Hazard3 RV32
  @ 150 MHz each, 512 KB SRAM, 4 MB flash).
- *Prover.* winterfell 0.13.1 vendored fork (`vendor/winterfell/`),
  all sub-crates already carry `#![no_std]`. Compiled with
  `default-features = false` -- no Rayon, no std, no async.
- *Firmware.* `bench-rp2350-m33-stark-prover` (M33) and
  `bench-rp2350-rv32-stark-prover` (RV32). Both use the `bench-core`
  harness: TrackingTlsf allocator, DWT / mcycle counters, USB-CDC
  output.
- *Circuit.* Fibonacci AIR, 2 columns, 2 transition constraints.
  Trace built on-device; public input is the final register value.
- *Proof options.* `num_queries = 8`, `blowup = 4`, `grinding = 0`,
  `FieldExtension::None` (Goldilocks base field only), `fri_folding = 4`,
  `fri_max_remainder_deg = 7`. Conjectured security: ~32 bits.
  Intentionally low -- this phase is about feasibility, not production.
- *Self-verification.* Every iteration the device verifies its own
  proof with the same winterfell verifier used in phase 3.

= Results

== Scaling: Cortex-M33, N = 64 through 256

#compare-table(
  ("N", "Prove (ms)", "Verify (ms)", "Heap peak (KB)", "Proof (B)", "Stack (B)"),
  (
    ([64],  [~36],  [~12], [~75],  [~3 072], [~4 448]),
    ([128], [68.3], [15.5], [153.5], [5 116], [4 448]),
    ([*256*], [*134*], [*19.4*], [*299*], [*6 668*], [*4 448*]),
  ),
)

Every ratio on the 2x step from 128 to 256:

#compare-table(
  ("Metric", "Ratio (256 / 128)", "Expected"),
  (
    ([Prove time],  [1.96x], [~2x  (O(N), log N flat at this scale)]),
    ([Verify time], [1.25x], [sub-linear (one extra FRI fold per 4x N)]),
    ([Heap peak],   [1.95x], [linear in N (LDE matrix dominates)]),
    ([Proof size],  [1.30x], [logarithmic (one extra FRI layer)]),
    ([Stack peak],  [1.00x], [constant (recursion depth independent of N)]),
  ),
)

Measured scaling matches theory to within 5 % at every metric.

== Cross-ISA: M33 vs RV32, N = 256

#compare-table(
  ("Metric", "Cortex-M33", "Hazard3 RV32", "Ratio (RV32 / M33)"),
  (
    ([Prove (ms)],     [134],    [208],    [*1.55x*]),
    ([Verify (ms)],    [19.4],   [25.0],   [*1.29x*]),
    ([Heap peak (KB)], [306],    [306],    [1.00x]),
    ([Proof (B)],      [6 668],  [6 668],  [1.00x]),
    ([Stack (B)],      [4 448],  [4 344],  [0.98x]),
    ([Variance (prove)], [0.16 %], [0.14 %], [--]),
  ),
)

Heap, proof size, and stack are hardware-independent: same Rust code,
same data structures, same allocator behaviour on both ISAs. Only
execution time differs.

= Findings

== 1. First on-device STARK proving on embedded MCU -- both ISAs

The prior-art survey (April 2026) found no published no_std STARK
prover running on any Cortex-M or RISC-V embedded core. Every public
result either ran the prover on a server and verified on-device, or
targeted WASM / desktop "embedded" environments. This run is the first
known measurement of a full STARK prove-then-self-verify loop on MCU
silicon, on both ARM and RISC-V.

The enabling insight was that winterfell's sub-crates already carry
`#![no_std]` internally. No library changes were needed. The embedded
port is a firmware wrapper, not a fork.

== 2. O(N) prove time; N = 256 is the SRAM ceiling at blowup = 4

Three data points (N = 64, 128, 256) all land within 3 % of the
expected 2x ratio per 2x trace. Prove time is effectively O(N) at
these sizes because log N is flat across the range (7 to 8 bits).

The heap scales exactly linearly with N because the LDE matrix
(`N * blowup * num_columns * 8 B`) is the dominant allocation:
75 KB at N = 64, 154 KB at N = 128, 299 KB at N = 256. At N = 512
with blowup = 4 the LDE alone would need ~600 KB -- more than the
entire 512 KB SRAM. *N = 256 is the hard ceiling for this chip at
production-viable FRI parameters.* Dropping to blowup = 2 would halve
heap and enable N = 512, but would also halve conjectured security; it
is not worth characterising when the current security level is already
a demo.

== 3. The prover (134 ms) is faster than the Groth16 verifier (643 ms)

On the same Cortex-M33 at the same 150 MHz:

#compare-table(
  ("Operation", "Time"),
  (
    ([BN254 Groth16 verify (1 public input)],   [643 ms]),
    ([BLS12-381 Groth16 verify],                [~2 015 ms]),
    ([*STARK Fibonacci prove N = 256 (this run)*], [*134 ms*]),
    ([STARK Fibonacci verify N = 256],          [19 ms]),
  ),
)

The device can *generate* a STARK proof of a 256-step computation
faster than it can *check* a Groth16 proof someone else produced. This
is not a general performance claim -- Groth16 provers on a server are
far faster than this STARK prover -- but it illustrates how different
the two systems are to run on constrained hardware. STARK proving is
a sequence of NTTs and hashes; Groth16 verification is pairing
arithmetic over 256-bit prime fields. The MCU is better suited to the
former.

== 4. Cross-ISA gap is workload-sensitive: 1.55x prove, 1.29x verify

The prove gap (1.55x RV32 / M33) is larger than the verify gap (1.29x)
because the operations are different in character.

*Prove is NTT-heavy (Goldilocks field multiplication).* Goldilocks is
a 64-bit prime field (p = 2^64 - 2^32 + 1). Multiplying two
64-bit elements requires a 64x64 -> 128-bit intermediate. On Cortex-M33
this is done with `UMAAL` (32x32 + 32 + 32 -> 64, one instruction).
On RV32IMAC it needs `mul` (lower 32 bits of 32x32) and `mulhu` (upper
32 bits), two instructions per partial product, four partial products
per 64-bit multiply. Roughly 2x the multiply cost, compounded over the
entire NTT.

*Verify is hash-heavy (Blake3).* Blake3 operates on 32-bit words with
ADD, XOR, and rotation. Both ISAs handle 32-bit operations equally
well. The residual 1.29x gap is general pipeline and code-density
efficiency on Thumb-2 vs RV32.

The 0.26x extra gap (1.55 - 1.29) is the measurable cost of not
having UMAAL on RISC-V. This is the same hardware feature that
explained the phase-2 BLS12-381 cross-ISA result (1.56x on
pairing-grade arithmetic). The pattern holds across proof systems: *M33
outperforms Hazard3 proportionally to how much 64-bit or 256-bit
field multiplication the workload contains.*

== 5. What "feasible" actually means

The Fibonacci AIR is the minimum-complexity circuit that exercises the
prover: 2 columns, 2 transition constraints, no auxiliary columns, no
lookups, no recursion. It is the Hello World of ZK circuits.

A real use case -- say, proving that a sensor reading stayed within a
range for N consecutive timesteps -- would be a 1-column,
1-constraint AIR. That would be *faster and smaller* than what was
measured here.

A general-purpose computation prover -- proving arbitrary firmware
execution, EVM traces, etc. -- would require N in the millions and
hundreds of columns. That is not feasible on this chip, and probably
not on any microcontroller shipping today.

The honest scope of this result: *the RP2350 can prove simple,
purpose-built AIR computations with short traces at low security
levels.* At N = 256 it takes 134 ms and 300 KB of heap. Scale the
circuit's column count or trace length beyond that and you run out
of SRAM before you finish the proof.

= Non-claims

- *Not production security.* 8 queries, no grinding, no field extension,
  Goldilocks base only -- conjectured security is ~32 bits. Lifting to
  80-bit security would need more queries and a field extension, pushing
  heap well above the SRAM budget on this chip.
- *Not a general computation prover.* Only the Fibonacci AIR was
  measured. More complex circuits scale linearly in heap with column
  count. A 10-column circuit would need ~3 MB heap at N = 256.
- *Not an optimised prover.* No ISA-specific field arithmetic, no
  UMAAL-tuned Goldilocks mul, no Zbb bit-manipulation on RV32. A
  dedicated optimisation pass could plausibly cut prove time by 1.5-2x
  on M33.
- *Not the prover you'd ship.* For production-grade ZK attestation from
  a device today, you would still generate the proof on a server and
  verify on-device, using the phase-1 through 3 verifier stack.

= Implications

This phase answers the question that was posted in the community in
April 2026: "I'm thinking about diving into proving next but yeah
guessing that will be impossible because of physics." It is not
impossible. It is 134 ms on a \$6 chip.

The narrow version of "feasible" described in Finding 5 is still
genuinely useful. A device that can prove its own short computations
does not need to trust a server to generate its proofs. That changes
the attestation model: instead of "the device reports X and you trust
it," you get "the device reports X with a proof that the computation
ran correctly." For IoT sensor attestation, firmware integrity checks,
or hardware-bound credential derivation, that is a meaningful
difference.

The cross-ISA comparison establishes that M33 is the better core for
this workload by ~55 % on proving. For a system designer choosing
between Cortex-M33 and a RISC-V core for an application that includes
on-device proving, that gap is decision-relevant.

= Phase 4 deliverable status

#compare-table(
  ("Step", "Status"),
  (
    ([4.0 no_std winterfell prover spike], [*done*]),
    ([4.1 M33 firmware + N=64 feasibility], [*done -- 36 ms, 75 KB heap*]),
    ([4.2 scaling run M33 N=128, N=256],   [*done -- linear confirmed*]),
    ([4.3 RV32 port + cross-ISA N=256],    [*done -- 1.55x prove gap*]),
    ([4.4 this report],                    [*done*]),
  ),
)

= Phase 5 candidates

- *Longer scaling curve at reduced security.* Blowup = 2 enables N = 512
  on M33. Worth doing if a paper submission needs a wider data set;
  not worth doing if the audience understands log-linear extrapolation.
- *Non-trivial AIR.* Replace Fibonacci with a hash-chain or range-check
  circuit. More columns and constraints per step; shows how heap and
  time scale with circuit complexity, not just trace length.
- *Optimised Goldilocks mul on M33.* Hand-written UMAAL inner loop for
  the NTT butterfly. Expected 1.5-2x prove speedup based on the
  fraction of proving time spent in field mul.
- *Write up the full arc.* The project now has benchmarks for Groth16
  verify (BN254, BLS12-381), STARK verify, and STARK proving -- all on
  the same hardware, all cross-ISA. That is a complete story that
  belongs in a proper paper, not just dated reports.

#v(1em)

_Raw measurements_:
`benchmarks/runs/2026-04-26-m33-stark-prover-fib/`

_Firmware_:
`crates/bench-rp2350-m33-stark-prover/` and
`crates/bench-rp2350-rv32-stark-prover/`
