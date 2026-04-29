# PQ-Semaphore verifier spike: winterfell vs Plonky3

**Date:** 2026-04-29
**Status:** decision recorded, recommendation: **path 2 (Plonky3)**
**Open question this resolves:** § 3.1 of `research/reports/2026-04-29-pq-semaphore-scoping.typ`

## Question

The PQ-Semaphore milestone uses Poseidon2-`BabyBear` (audited at
`crates/zkmcu-poseidon-audit`, parameters bit-identical to Plonky3's
published instance). Two viable verifier frameworks:

1. **Winterfell.** The existing `zkmcu-verifier-stark` crate uses
   `winterfell = "0.13"`. Already runs on both MCU targets
   (`bench-rp2350-m33-stark-prover-bb`, phase 3.3).
2. **Plonky3.** Already vendored at `vendor/Plonky3/`, but currently
   used only by the host-only `zkmcu-poseidon-audit` crate. Never
   compiled for `thumbv8m.main-none-eabihf` or
   `riscv32imac-unknown-none-elf` until this spike.

Decision criteria: hash availability, field availability, dep-closure
weight, build-cleanness on the two MCU targets, and porting risk
relative to the audit work already done.

## Method

Standalone scratch crate at
`research/notebook/2026-04-29-pq-semaphore-verifier-spike/` with path
deps into `vendor/Plonky3` for the verifier-side stack
(`p3-uni-stark`, `p3-baby-bear`, `p3-fri`, `p3-poseidon2`,
`p3-symmetric`, `p3-merkle-tree`, `p3-commit`, `p3-challenger`,
`p3-matrix`, `p3-air`, `p3-field`). `no_std`, `extern crate alloc`,
no Cargo features enabled.

Compared dep-closure size against the winterfell dep-fit spike from
2026-04-23 (`research/notebook/2026-04-23-stark-dep-fit/`).

## Findings

### Plonky3 builds `no_std` clean for both targets

Zero patches, zero feature flips, default deps.

| Target                          | `cargo build --release` | Result |
|---------------------------------|-------------------------|--------|
| `thumbv8m.main-none-eabihf`     | 6.53 s cold             | clean  |
| `riscv32imac-unknown-none-elf`  | 2.07 s warm             | clean  |

`p3-uni-stark` declares `#![no_std]` + `extern crate alloc`. The
verifier (`uni-stark/src/verifier.rs`) imports only `alloc`, `p3-*`,
`itertools`, and `tracing` — no std-only path. Same for
`p3-baby-bear`, `p3-poseidon2`, `p3-fri`, etc.

Verify entry point is a single function:
```rust
pub fn verify<SC, A>(
    config: &SC,
    air: &A,
    proof: &Proof<SC>,
    public_values: &[Val<SC>],
) -> Result<(), VerificationError<PcsError<SC>>>
```

Standard for a STARK verifier: pass a config (defines hash, field,
extension, FRI params), an AIR, a deserialised proof, and public
inputs.

### Winterfell has no `BabyBear` and no Poseidon2

`vendor/winterfell/math/src/field/` has only:
- `f128` (128-bit prime)
- `f62`
- `f64` (Goldilocks)

No `BabyBear`, no Mersenne-31, no KoalaBear.
`vendor/winterfell/crypto/src/hash/` has only `blake`, `rescue`, `sha`.
No Poseidon, no Poseidon2.

Path 1 ("stay on winterfell, port the audited Poseidon2") therefore
expands to: port BabyBear as `winterfell::math::StarkField`, port
the quartic extension as `ExtensibleField<4>`, port Poseidon2 as
`winterfell::crypto::ElementHasher`, then **re-audit all three**
because the audit at `crates/zkmcu-poseidon-audit` validates only
Plonky3's exact field + hash code paths. A port introduces a fresh
attack surface (Montgomery reduction, extension multiplication,
permutation byte order) that the audit doesn't cover.

### Dep-closure delta is modest

| Stack                           | Unique deps (thumbv8m) |
|---------------------------------|-----------------------:|
| Plonky3 verifier-side           | 45                     |
| Winterfell (umbrella)           | 34                     |

Plonky3 pulls ~30 % more dep crates, mostly its own `p3-*` modular
split (one crate per field, one per hash, one per FRI layer, etc.).
Not a blocker. Compile time on warm cache is comparable.

### Audit alignment is the dominating consideration

The audit milestone is the foundation that the entire PQ-Semaphore
narrative rests on. It validates Plonky3's *exact* code paths:
- `p3-baby-bear::BabyBear` for the field
- `p3-poseidon2::Poseidon2ExternalLayer` + `Poseidon2InternalLayer`
  for the hash
- The exact round-constants and round counts (`R_F = 8`, `R_P = 13`)

Path 2 (Plonky3 verifier) inherits all of this without a single
byte of porting. Path 1 invalidates the audit's coverage and
forces a second audit pass for the ported field + hash.

Re-auditing a ported BabyBear + Poseidon2 is plausibly 1 -- 2 weeks
of work on top of the AIR build, which alone is the ~4-week
PQ-Semaphore milestone scope. Path 1 effectively pushes the
milestone out by 25 -- 50 %.

## Recommendation

**Take path 2. Add `crates/zkmcu-verifier-plonky3` as a new sibling
crate.**

Rationale (in priority order):

1. *Audit alignment.* Reusing Plonky3's exact field and hash code
   means the audit's permutation diff test
   (`zkmcu-poseidon-audit::tests::perm_diff`, 10 cases including
   200-input random stress) directly covers what runs on the MCU.
   No re-audit. No second-source verification dance.
2. *Build risk.* The spike proved Plonky3 builds clean for both
   MCU targets, no patches needed. Winterfell-with-ported-BabyBear
   has never been built and the porting work is non-trivial.
3. *Dep-closure cost.* +11 crates. Not material for a standalone
   verifier crate. Both stacks fit comfortably in the firmware
   build.

The two costs of path 2:

- *Two STARK verifiers in the source tree.* `zkmcu-verifier-stark`
  (winterfell, used by the existing Fibonacci / threshold benches)
  stays. `zkmcu-verifier-plonky3` (new, used by PQ-Semaphore) is
  added alongside it. Each firmware crate picks one. This is the
  same pattern as `zkmcu-verifier` (BN254) coexisting with
  `zkmcu-verifier-bls12` — accepted overhead.
- *Plonky3 verify is unbenchmarked on this hardware.* The
  prediction in the scoping report (§ 5) extrapolates from
  winterfell's measured costs. A first measurement may move the
  estimate. Falsification criteria in the scoping report account
  for this with a wide interval.

## Concrete next steps

1. **Land a commit for the spike + scoping doc.** This notebook
   entry plus `research/reports/2026-04-29-pq-semaphore-scoping.typ`
   plus the `justfile` entry. Single commit, no firmware changes
   yet.
2. **Add `crates/zkmcu-verifier-plonky3` skeleton.** Mirror
   `zkmcu-verifier-stark`'s `lib.rs` structure: AIR-specific
   `verify_<name>(proof_bytes, public_bytes)` entry points,
   `no_std` from day one, deps via path on `vendor/Plonky3`.
3. **First AIR: a minimal Poseidon2 hash-chain check.** Smaller
   than full PQ-Semaphore but uses the audited hash directly, so
   it doubles as a sanity check on the verifier wiring. Bench it
   on M33 + RV32. This becomes the new "Fibonacci" baseline for
   the Plonky3 stack and the anchor measurement gets sharper than
   the scoping doc's extrapolation.
4. **Then PQ-Semaphore AIR proper.** Merkle path + nullifier +
   scope binding, per the scoping doc.

Step 3 is roughly a week and substantially de-risks step 4. Worth
inserting between scoping and the headline AIR.

## Artifacts

- This notebook entry: `research/notebook/2026-04-29-pq-semaphore-verifier-spike.md`
- The standalone build crate:
  `research/notebook/2026-04-29-pq-semaphore-verifier-spike/`
  (gitignored — the directory holds `Cargo.lock` and a `target/`).
  Listed in `.gitignore` if not already; check before committing.
