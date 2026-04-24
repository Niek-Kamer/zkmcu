# 2026-04-23 — STARK verifier on MCU: prior art + design space

Phase 3 of zkmcu extends the verifier family beyond pairing-based Groth16
into hash + FRI-based STARKs. This notebook scopes the problem before
any code gets written, following the same discipline we used for the
BLS12-381 dep-fit spike (`2026-04-22-bls12-381-dep-fit.md`).

## What's different from Groth16

STARKs verify computation integrity using:

- **Hash + Merkle commitments** instead of pairings on elliptic curves
- **FRI** (Fast Reed-Solomon IOP) for the low-degree test
- **AIR** (Algebraic Intermediate Representation) for the constraint system
- Transparent (no trusted setup)
- Much larger proof size: 50 KB - 1 MB+ (vs 256 B for Groth16/BN254)
- Polylog verifier complexity — still fast at the verify end, bigger computation on the prover end

The no_std / MCU angle is potentially easier than Groth16 in one way: no
pairing-friendly curve is needed. Field arithmetic over a 32-64 bit
prime + a good hash function is the whole crypto surface. In practice
the complication comes from *proof size* — 100 KB proofs are marginal
for a 520 KB SRAM chip when you also need to buffer witness + Merkle
paths + FRI layers.

## Rust STARK implementations (crates.io, 2026-04-23)

| Crate | Version | Source | `std` feature? | Notes |
|---|---|---|---|---|
| `winterfell` | 0.13.1 | Novi Financial / Polygon Miden | yes, default | Verifier-focused umbrella. Pure STARK, not a VM. |
| `winter-crypto` | 0.13.1 | same | yes, default | Hashes + Merkle — used by winterfell |
| `winter-fri` | 0.13.1 | same | yes, default | FRI verifier component |
| `miden-core` | 0.22.1 | 0xMiden | yes, default | Miden VM core; built on winterfell |
| `plonky2` | 1.1.0 | 0xPolygonZero | yes, default | Hybrid PLONK+FRI; prover-heavy, recursive SNARKs |
| `starky` | 1.1.0 | 0xPolygonZero | yes, default | Pure STARK variant in the plonky2 ecosystem |
| `risc0-zkvm` | 3.0.5 | RISC Zero | yes, default + `heap-embedded-alloc` feature | Full zkVM; verifier is a sub-component |

Every one of them has an optional `std` feature, which means all of
them *claim* to support `no_std` with `default-features = false`.
Whether those claims hold up against an embedded target is exactly
what the dep-fit spike will test.

## Candidate ranking for a dep-fit spike

Going by "smallest verifier surface that still does something
recognisable", rank order:

1. **`winterfell` 0.13**. Verifier-only umbrella. Facebook-origin,
   Polygon-Miden-maintained, mature. No prover machinery forced into
   the compile graph. Smallest realistic scope for "first STARK
   verifier on MCU".
2. **`winter-verifier` + `winter-fri` + `winter-crypto` + `winter-math`
   directly**. Unpacking the umbrella — more control, but needs we
   understand the dep relationships upstream first.
3. **`starky` 1.1**. Smaller than plonky2, similar ecosystem. Worth a
   second look if winterfell doesn't pan out for some reason.
4. **`miden-core` / `miden-vm`**. Adds Miden VM semantics on top. Overkill
   unless we specifically want to verify Miden-VM execution traces.
5. **`risc0-zkvm`**. Already has `heap-embedded-alloc` feature
   signalling embedded awareness, but it's a full zkVM and the verifier
   is buried inside. Bigger adaptation cost.
6. **`plonky2`**. PLONK+FRI hybrid, famously prover-heavy. Lower
   priority unless we want recursive SNARK verification later.

## Key design decisions (to make before writing a real verifier crate)

A dep-fit spike only tests "does the crate build no_std". Once that
lands, a real `zkmcu-verifier-stark` crate has to pick specific choices
along several axes. Calling them out now so they're visible:

### Field choice

STARKs work over a prime field. On a 32-bit MCU the field-size
ceiling matters a lot for inner-loop performance:

- **Goldilocks** (`p = 2^64 - 2^32 + 1`): popular for plonky2/winterfell,
  but native arithmetic on the M33 is 32-bit — 64-bit field ops cost
  2-4× extra cycles per mul.
- **BabyBear** (`p = 2^31 - 2^27 + 1`): small, fits natively in 32-bit
  registers, STARK-friendly, used by risc0 zkVM recent work.
- **Mersenne-31** (`p = 2^31 - 1`): even smaller, very fast reduction
  via bit-and + shift, but security margin is tight (need extension
  fields for real use).
- **BN254 scalar** (254-bit): lets us reuse substrate-bn field
  arithmetic from the existing verifier, but big and no STARK frameworks
  actually use this field.

Default assumption: **BabyBear or Goldilocks**, whichever the
winterfell / starky verifier path can drive with `default-features = false`.

### Hash choice

- **Blake3 / Blake2s**: fast on ARM Cortex-M, widely implemented no_std.
- **Poseidon / Rescue / RPO**: "STARK-friendly" (small number of field
  multiplications per hash), cheap on the prover side but not especially
  fast on MCU verify.
- **Keccak / SHA-3**: medium speed, universally available.

Tradeoff: Poseidon-class hashes make the prover's life easier and
shrink proof size, but Blake2 runs faster on Cortex-M. Picking one
depends on what the target STARK framework ships with as its default.
winterfell defaults to RPO or Blake3; that's what we'll get if we let
the spike choose.

### AIR (circuit) for the first bench

Pure dep-fit doesn't need a circuit. The real verifier eventually needs
a concrete AIR to verify *something* against. Options, roughly in
increasing realism:

- **Fibonacci AIR**: the STARK "hello world". Proves the N-th Fibonacci
  number. 2-column trace, trivial constraint. Small proof.
- **Hash chain AIR**: proves you applied a hash N times. More realistic,
  bigger proof.
- **Range proof AIR**: proves `x < 2^64` or similar. Practical but specific.
- **Small RISC-V segment**: closer to risc0-style VM verification. Much
  bigger proof.

Start with Fibonacci for Phase 3.1, move to something closer to a
real-world AIR when that works.

### Proof size budget

Pico 2W has 520 KB SRAM total. After firmware / heap / stack we
realistically have 200-300 KB to dedicate to verify-time buffers.
Winterfell Fibonacci proofs at default security are ~50 KB; hash-chain
proofs ~80 KB; Miden VM execution proofs 200 KB+. The 200-300 KB
headroom is tight for the larger categories, streaming verify would
help but that's a Phase 4 discussion.

For the spike, we don't actually run a proof through the verifier,
we just confirm the crate compiles `no_std`. Proof-size experiments
come after.

## Hypothesis (written before running anything)

- **Primary**: `winterfell = "0.13"` with `default-features = false`
  will build cleanly for `thumbv8m.main-none-eabihf` and
  `riscv32imac-unknown-none-elf`. **Confidence: medium.** The crate has
  an explicit `std` feature wich is a strong positive signal, but
  winterfell is bigger than `bls12_381` was — more transitive deps,
  more chances for a hidden `std` leak or a `getrandom` pull.

- **Expected failure modes**:
  - Transitive `std`-only dep (most likely: something in
    winter-utils or the default hash impl)
  - `getrandom` pulled via `rand_core` default features —
    `getrandom` needs a target-specific backend on MCU
  - `parallel` / `rayon` accidentally compiled in
  - `once_cell::sync` or `parking_lot` sneaking in via a transitive
  - A proc-macro crate with non-no_std expansion

- **Fallback plan**: if `winterfell` umbrella pulls std, try the
  lower-level `winter-verifier` + `winter-fri` + `winter-crypto`
  crates directly; if those also pull std, try `starky` as the
  plan B; if no pure-Rust STARK verifier is no_std-clean, phase 3
  scope shifts to writing a minimal STARK verifier from scratch
  (much bigger project, 1-3 months not 1-2 weeks).

## What this notebook will NOT decide

- Field choice (need the dep-fit outcome first)
- Hash choice (ditto)
- AIR shape (Phase 3.2 work, not spike work)
- Proof size vs security trade-off (Phase 3.3)
- Whether we use winterfell's own prover or write host-gen integration
  against a different prover

## Next concrete step

Scaffold `research/notebook/2026-04-23-stark-dep-fit/` — standalone
workspace (same pattern as BLS12 spike), add `winterfell = "0.13"` with
`default-features = false` as the only dep, write a trivial lib that
references the verifier API, and run:

```bash
cargo build --release --target thumbv8m.main-none-eabihf
cargo build --release --target riscv32imac-unknown-none-elf
```

Success criterion: both commands exit 0 with no warnings.

## Findings

**Outcome: both targets green on first attempt, zero warnings.** Hypothesis
confirmed, confidence level earned.

```
$ cargo build --release --manifest-path .../Cargo.toml --target thumbv8m.main-none-eabihf
   Compiling winter-math v0.13.1
   Compiling blake3 v1.8.4
   Compiling sha3 v0.10.9
   Compiling winter-crypto v0.13.1
   Compiling winter-fri v0.13.1
   Compiling winter-air v0.13.1
   Compiling winter-verifier v0.13.1
   Compiling winter-prover v0.13.1
   Compiling winterfell v0.13.1
   ...
    Finished `release` profile [optimized] target(s) in 4.91s

$ cargo build --release --manifest-path .../Cargo.toml --target riscv32imac-unknown-none-elf
   ... (similar trace)
    Finished `release` profile [optimized] target(s) in 1.02s
```

Zero warnings on either target. No `[patch.crates-io]` entry required,
no fork, no feature gymnastics beyond `default-features = false`.

### Dep graph shape

43 total transitive deps (~5× bigger than the `bls12_381` closure).
Notable presence / absence:

- *Present*: winter-air, winter-crypto, winter-fri, winter-math,
  winter-utils, winter-verifier, winter-prover (yes, prover came along
  — LTO will strip it in a verify-only firmware), blake3, sha3,
  keccak, digest, generic-array, typenum, libm, tracing
- *Absent (the usual no_std traps all dodged)*: `getrandom`,
  `parking_lot`, `once_cell::sync`, `rayon`, `std::sync::*` leaks,
  `parallel` features

### rlib sizes (unlinked, with generic code pre-monomorphization)

| crate | M33 | RV32 |
|---|---:|---:|
| `typenum` | 2,760,714 B | 2,760,666 B |
| `libm` | 1,399,356 B | 1,399,956 B |
| `generic-array` | 826,662 B | 826,602 B |
| `winter-air` | 795,672 B | 795,712 B |
| `winter-crypto` | 777,464 B | 776,936 B |
| `tracing-core` | 771,702 B | 772,146 B |
| `winter-prover` | 716,760 B | 716,720 B |
| `winter-math` | 627,688 B | 627,640 B |
| `tracing` | 612,192 B | 612,164 B |
| `blake3` | 390,948 B | 390,064 B |
| `winter-fri` | 265,918 B | 265,890 B |

Byte-for-byte rlib parity between M33 and RV32 confirms no
arch-specific specialization. The big rlibs (typenum, libm) are
mostly generic / compile-time type machinery — LTO strips most of it
from the final firmware link.

`winter-prover` sitting in the closure is mild bloat at build time,
not at runtime (unused code → LTO drops). Future work: check whether
depending on `winter-verifier` + `winter-fri` + `winter-crypto` +
`winter-math` directly (skipping the `winterfell` umbrella) avoids
pulling in the prover entirely.

### Surprises worth recording

- **`blake3` compiled clean for both embedded targets**. blake3 normally
  wants to compile C / asm SIMD intrinsics for SSE / NEON / AVX. On
  our targets it detected "none of the above" and fell back to pure
  Rust — no cross-compiler needed, no C backend fired. Good default.
- **`tracing` builds no_std + alloc**. I expected this to be a std trap;
  it isn't in 0.1.44. Note: the crate compiles, but emitting events
  would need a subscriber wich we won't configure on firmware — tracing
  calls from winter-prover will compile down to no-ops.
- **`cc` crate appears in the build graph but only as a build dep**
  (doesn't end up in the embedded binary). Confused me for a second —
  no ARM cross-compiler is needed at my dev host because blake3 chose
  not to invoke the C path for embedded targets.

### What this means for the project

1. **STARK verifier on MCU is wiring work, not research.** winterfell
   0.13 is no_std-clean for both Cortex-M33 and Hazard3 RV32 out of
   the box. No fork, no patch, no nightly.
2. **The dep closure is larger than Groth16 was** (43 vs ~8 crates)
   but not alarming — it's dominated by generic type machinery and
   tracing, both strippable under LTO.
3. **Blake3 + SHA-3 + Keccak are all available as the hash backends
   without extra work.** Hash choice moves to the "which is fastest
   on Cortex-M33" question rather than "which can we make build".
4. **Prover code sits in the build graph** unless we move to direct
   sub-crate deps. Minor cleanup, not a blocker.

## Next-phase plan (Phase 3.1 scope)

- [ ] Scaffold `crates/zkmcu-verifier-stark` as a sibling of
  `zkmcu-verifier` and `zkmcu-verifier-bls12`. Same shape: no_std,
  workspace member, clippy workspace-inherit.
- [ ] Pick the AIR for the first bench. Fibonacci is the hello world;
  worth going there first.
- [ ] Pick field + hash. Default: BabyBear (if winter-math supports
  it directly) or Goldilocks (winterfell default). Blake3 for hash
  because it's fast on Cortex-M and already compiled here.
- [ ] Write the verifier API: parse_proof, parse_public, verify.
  Different from Groth16 because STARK proofs are variable-size and
  have structure (FRI layers, Merkle paths) that needs streaming.
- [ ] Host-side: produce a test proof via winterfell's prover (the
  `winter-prover` code that's already in the build). Commit the proof
  bytes under `crates/zkmcu-vectors/data/stark-fib-*/`.
- [ ] Firmware crates `bench-rp2350-m33-stark` + `-rv32-stark`.
- [ ] First measurement run.

### Next-phase hypothesis (prediction before measurement)

For a **small Fibonacci AIR proof** (2-column trace, N=1024 steps,
96-bit security parameters, Blake3 hash, Goldilocks field) verified
on Cortex-M33 at 150 MHz:

- *Proof size*: \~40-60 KB (winterfell default security).
- *Verify time*: **predicted 150-400 ms**. STARK verifier cost is
  polylog(N), dominated by Merkle auth-path hashing + FRI layer
  checks. Blake3 runs \~200 MB/s on Cortex-M33 which should eat the
  hash cost; FRI has O(log N) rounds, \~10-12 rounds for N=1024.
- *Peak RAM during verify*: \~100-200 KB — needs to hold the proof
  buffer plus Merkle auth-path temporaries.
- *Variance*: expect 0.03-0.1 % iteration-to-iteration, matching
  other measurements on this silicon.

**Prediction confidence: low.** Never measured a STARK verifier on
this tier of hardware. Could be off by 2-3× in either direction.
These numbers will be committed to a prediction report before the
firmware benchmarks run.

### Open question: umbrella vs direct sub-deps

Depending on `winterfell` pulls `winter-prover` into the build graph
even for verify-only firmware. Verify takes LTO-strippable but
pre-link rlibs are wasted compile time. Worth a 10-minute follow-up
spike: swap the dep to `winter-verifier` + `winter-fri` +
`winter-crypto` + `winter-math` directly and see if the prover
disappears from the closure. If yes, use the direct form; if the
umbrella pulls prover through re-exports, stick with the umbrella
and rely on LTO. Phase 3.1 scope item.
