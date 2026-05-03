# Plan: PQ-Semaphore hardening — fuzz, constant-time, self-audit

**Created:** 2026-04-30 (end of day, post-Phase-F)
**Status:** drafted, not started.

This plan picks up where `2026-04-29-security-128bit-plan.md` left off. Phases A–F closed the FRI parameter side: the dual-hash PQ-Semaphore verifier is now at **128 conjectured FRI bits per leg**, 186-bit hash floor per leg, dual-hash composition, ~1611 ms M33 / 2041 ms RV32, 384 KB heap, 337 KB total proof bytes, `ok=true` on every iteration on both ISAs. Bench artifacts: `benchmarks/runs/2026-04-30-{m33,rv32}-pq-semaphore-dual-q17/`.

That closes the parameter-tuning question. The remaining hardening is **not** about FRI math; it's about catching bugs the parameters can't protect against (parser bugs, AIR-soundness bugs) and closing the timing oracle Phase C deliberately created. After this lands, the project is ready for an external audit.

This plan was written *before* any of Phase G–I work ran, in the same falsifiable-prediction style as the 128-bit plan. Anyone can `git blame` the predicted bands and confirm they were not back-fitted to the result.

## Starting point (post-Phase-F)

- **Verifier:** `crates/zkmcu-verifier-plonky3/src/{pq_semaphore.rs, pq_semaphore_blake3.rs, pq_semaphore_dual.rs}`
- **Host-gen:** `crates/zkmcu-host-gen/src/{pq_semaphore.rs, pq_semaphore_dual.rs}` (re-uses verifier `make_config` — single source of truth for FRI constants)
- **Firmware:** `crates/bench-rp2350-{m33,rv32}-pq-semaphore-dual` (Phase E.1 / F headline harness), plus `…-pq-semaphore-reject` (Phase C mutation harness)
- **Vectors:** `crates/zkmcu-vectors/data/pq-semaphore-d10-dual/{proof_p2.bin, proof_b3.bin, public.bin}` (regenerated 2026-04-30 against 16+17 grinding)
- **Mutation patterns:** `crates/zkmcu-vectors/src/mutations.rs` (M0–M5)
- **Existing fuzz harness:** `fuzz/Cargo.toml` already contains `bn254_*`, `bls12_*`, `stark_parse_proof` targets. Pattern is libFuzzer + nightly cargo-fuzz, seeds under `fuzz/seeds/<target>/`. Default 60s smoke per target via `just fuzz <target>`; full sweep via `just fuzz-smoke`.

Hardware loop is **manual BOOTSEL flash via Pi 5**, hand-delivered SCP + picotool block to the user. See top-level `CLAUDE.md` for the canonical sequence.

---

## Phase order and dependencies

```
G (fuzz)          →  H (constant-time)  →  I (self-audit)  →  [external audit ask]
parser bugs           timing oracle           soundness review     (separate workstream)
caught first          closed before           reviews the
                      audit reads code        post-G+H tree
```

Sequential because Phase I reviews the code shape that lands after G and H. External audit ask is a separate workstream after Phase I closes (cost, vendor selection, scope letter — not in this plan).

Production-silicon hardening (signed-boot via OTP, SWD lock, hardware AES, glitch detectors, optional secure element) is **out of scope on the dev unit** and tracked separately. This plan is "single-chip, single open-hardware Pico 2 W, no fuse burns, no extra silicon".

---

## Phase G — fuzz coverage for the PQ-Semaphore parser path

**Cost:** ~1 day to write four targets + extend `just fuzz-smoke`. Then CI / nightly compute time only.
**Win:** Any bug in `parse_proof` / `parse_public` / postcard length-cap / dual-leg orchestration falls out before review. Closes the "MAX_PROOF_SIZE doesn't catch every malformed shape" risk.

### Predictions (falsifiable)

| Target | Predicted throughput | Predicted bug count (1-hour smoke) |
|---|---:|---:|
| `pq_semaphore_parse_proof_p2` | ≥ 5 000 exec/s | 0–2 |
| `pq_semaphore_parse_proof_b3` | ≥ 5 000 exec/s | 0–2 |
| `pq_semaphore_parse_public` | ≥ 50 000 exec/s | 0–1 |
| `pq_semaphore_dual_parse_and_verify` | ≥ 50 exec/s | 0–3 |

The `dual_parse_and_verify` target is naturally throughput-bound by host-side verify (~10–20 ms per accepted proof, libFuzzer rejects spend most of their time in parse only). Bug count is a soft prediction — the parsers are reused across phases and fairly mature, so a high count would be a model violation telling us the dual-leg orchestration introduced something the single-leg targets missed.

### Touchpoints

1. New entries in `fuzz/Cargo.toml`:
   ```toml
   [[bin]]
   name = "pq_semaphore_parse_proof_p2"
   path = "fuzz_targets/pq_semaphore_parse_proof_p2.rs"
   test = false
   doc = false

   [[bin]]
   name = "pq_semaphore_parse_proof_b3"
   path = "fuzz_targets/pq_semaphore_parse_proof_b3.rs"
   test = false
   doc = false

   [[bin]]
   name = "pq_semaphore_parse_public"
   path = "fuzz_targets/pq_semaphore_parse_public.rs"
   test = false
   doc = false

   [[bin]]
   name = "pq_semaphore_dual_parse_and_verify"
   path = "fuzz_targets/pq_semaphore_dual_parse_and_verify.rs"
   test = false
   doc = false
   ```

2. New `fuzz/fuzz_targets/<name>.rs` files. Each is a thin shim:
   ```rust
   #![no_main]
   use libfuzzer_sys::fuzz_target;
   use zkmcu_verifier_plonky3::pq_semaphore::parse_proof;

   fuzz_target!(|data: &[u8]| {
       let _ = parse_proof(data); // must never panic; Result is fine to discard
   });
   ```

   For `pq_semaphore_dual_parse_and_verify` the harness splits the input into a `(p2_bytes, b3_bytes, public_bytes)` triple via the standard libFuzzer length-prefix pattern (4-byte little-endian prefix per blob) and runs the full `pq_semaphore_dual::parse_and_verify`. Verify-time errors are fine — the property is "no panic on any input".

3. Seed corpora `fuzz/seeds/<target>/`:
   - Copy committed proofs as the canonical seed.
   - Add 6 mutation seeds derived from `crates/zkmcu-vectors/src/mutations.rs` M0–M5 (pre-mutated copies of each canonical proof).
   - libFuzzer will minimise + extend from these.

4. Extend `just fuzz-smoke` recipe in `justfile`:
   ```just
   fuzz-smoke SECS="10":
       just fuzz bn254_parse_vk {{SECS}}
       # ... existing targets ...
       just fuzz pq_semaphore_parse_proof_p2 {{SECS}}
       just fuzz pq_semaphore_parse_proof_b3 {{SECS}}
       just fuzz pq_semaphore_parse_public {{SECS}}
       just fuzz pq_semaphore_dual_parse_and_verify {{SECS}}
   ```

5. New justfile recipe for the longer per-target campaign (1-hour, configurable):
   ```just
   fuzz-pq-campaign SECS="3600":
       just fuzz pq_semaphore_parse_proof_p2 {{SECS}}
       just fuzz pq_semaphore_parse_proof_b3 {{SECS}}
       just fuzz pq_semaphore_parse_public {{SECS}}
       just fuzz pq_semaphore_dual_parse_and_verify {{SECS}}
   ```

### Success criteria

- All four targets compile and run via `just fuzz <target>` against their seed corpora without immediate crashes.
- `just fuzz-smoke 10` (10 s/target) completes clean.
- 1-hour campaign per target completes clean (zero crashes / hangs / OOMs).
- Any finding triaged into either:
  - a fix on the same branch (Critical / High), or
  - a `notes.md` under `fuzz/findings/<date>-<slug>/` with severity + reproducer (Medium / Low / Info).

### Artifacts

- 4 new `fuzz/fuzz_targets/*.rs` files
- 4 new `fuzz/seeds/<target>/` directories with canonical + mutation seeds
- Updated `fuzz/Cargo.toml` and `justfile`
- `research/notebook/<date>-phase-g-fuzz-results.md` summarising 1-hour campaign output, throughputs measured, any findings

---

## Phase H — constant-time verify path

**Cost:** ~3–5 days. New verify entry point + bench harness + on-silicon mutation re-run.
**Win:** Removes the timing oracle Phase C deliberately created. Phase C reject medians (M0 8.5 ms, M5 127 ms vs honest 1131 ms / Phase F 1611 ms) leak which check failed — for a personal hardware wallet that's a real side channel.

### Scope boundary (important — write this in `notes.md` up front)

This is **macro-scale CT**, not **instruction-level CT**.

- **Macro-scale:** every input takes the same wall-clock time (within ε) to produce a yes/no answer. Sufficient against a remote attacker who can only observe overall verify duration via USB CDC-ACM responses.
- **Instruction-level:** every BabyBear add and Poseidon2 round is branch-free on secrets. Defends against an attacker with cycle-resolved probing on the chip (lab equipment). Requires per-line review of vendored Plonky3 code. **Out of scope for Phase H** — explicit deferral, not implicit assumption.

### Predictions (falsifiable)

Mutation harness against the new CT path. Compare M33 medians to the honest verify time.

| Mutation | Phase C reject (M33, current) | CT reject (M33, predicted) |
|---|---:|---:|
| M0 header byte | 8.53 ms | **1611 ± 8 ms** (honest ± 0.5 %) |
| M1 trace commit | 8.58 ms | **1611 ± 8 ms** |
| M2 mid FRI | 13.14 ms | **1611 ± 8 ms** |
| M3 query opening | 44.11 ms | **1611 ± 8 ms** |
| M4 final layer | 44.09 ms | **1611 ± 8 ms** |
| M5 public byte | 126.87 ms | **1611 ± 8 ms** |
| honest | 1611.44 ms (Phase F) | **1611 ± 8 ms** |

Same prediction shape for RV32 anchored at 2041 ms.

If any reject median is more than ±0.5 % off the honest median on either ISA, the macro-scale CT property is violated and we redesign before continuing. If `range_pct` per pattern grows above 0.1 % we suspect data-dependent allocator behaviour in the drop-between path and investigate before claiming CT.

### Touchpoints

1. New module: `crates/zkmcu-verifier-plonky3/src/pq_semaphore_dual_ct.rs`. Public entry point:
   ```rust
   pub fn verify_constant_time(
       proof_p2: &[u8],
       proof_b3: &[u8],
       public: &[u8],
   ) -> bool
   ```
   Implementation rules:
   - No `?` propagation on the verify result. Both legs always run to completion.
   - Errors collapse into `false` via `let r1 = parse_and_verify_p2(...).is_ok(); let r2 = parse_and_verify_b3(...).is_ok(); r1 & r2` (note: bitwise `&`, not short-circuit `&&`).
   - Parse failures get a stand-in "always-false" check that still walks the verify path with placeholder data, so total cycle count doesn't depend on whether parse succeeded. (Detail: the placeholder must be a *static* canonical bad proof, not a function of `data`, to avoid second-order leakage.)
   - Heap-drop pattern from `pq_semaphore_dual.rs` is preserved — peak heap stays at 304 KB.

2. Phase C fast-fail entry point at `pq_semaphore_dual::parse_and_verify` is **kept**, not replaced. Both are public API. Document deployment guidance in the crate-level rustdoc:
   - `verify_constant_time` for personal hardware tokens, FIDO-class devices, anything where attacker-controlled probing is in the threat model.
   - `parse_and_verify` for relay endpoints, public-facing verifiers, anything where DoS resistance matters more than timing-leak resistance.

3. New firmware crates:
   - `crates/bench-rp2350-m33-pq-semaphore-dual-ct` — honest-path bench, 20 iterations, mirrors Phase F harness shape.
   - `crates/bench-rp2350-rv32-pq-semaphore-dual-ct` — same, RV32 target.
   - `crates/bench-rp2350-{m33,rv32}-pq-semaphore-ct-reject` — runs the M0–M5 mutation patterns through the CT entry point. Same shape as the existing `…-pq-semaphore-reject` crates.

4. New justfile recipes:
   ```just
   build-m33-pq-semaphore-dual-ct:
       cd crates/bench-rp2350-m33-pq-semaphore-dual-ct && cargo build --release

   build-rv32-pq-semaphore-dual-ct:
       cd crates/bench-rp2350-rv32-pq-semaphore-dual-ct && cargo build --release

   build-m33-pq-semaphore-ct-reject:
       cd crates/bench-rp2350-m33-pq-semaphore-ct-reject && cargo build --release

   build-rv32-pq-semaphore-ct-reject:
       cd crates/bench-rp2350-rv32-pq-semaphore-ct-reject && cargo build --release
   ```

### On-silicon flash sequence

Standard Pi 5 hand-delivered flow per `CLAUDE.md`. Two passes per ISA: honest-path bench then mutation harness. Capture under:

- `benchmarks/runs/<date>-{m33,rv32}-pq-semaphore-dual-ct/`
- `benchmarks/runs/<date>-{m33,rv32}-pq-semaphore-ct-reject/`

### Success criteria

- All seven medians (six rejects + honest) within ±0.5 % of each other on **both** ISAs.
- `range_pct` < 0.1 % per pattern (matches Phase F variance).
- Per-pattern `result.toml` includes `timing_oracle_residual_bits` field with derived bit-leakage estimate (Shannon entropy of the timing distribution conditioned on the mutation class). Target ≤ 1 bit per probe attempt.
- Honest-path verify time is within +1 % of Phase F (CT path can be slower, but not by much — most of the cost is unchanged).
- `notes.md` documents the macro-vs-instruction-level CT scope boundary explicitly.

### Trade-off (write this in `notes.md` up front)

CT path costs roughly +480 ms per reject vs Phase C fast-fail. Worst-case attacker DoS gain reverts to 1× — every probe is a full verify. This is the deliberate trade. Deployment guidance in rustdoc keeps both APIs available; downstream callers pick.

---

## Phase I — self-security-audit pass

**Cost:** ~3–5 days of structured review.
**Win:** Every soundness / witness-binding / transcript / wire-format hole that's catchable by careful reading is logged before an external auditor sees the code. External audit calendar time is expensive; pre-clearing the obvious findings is the highest-leverage prep step.

### Scope: "new stuff since the audited Plonky3 boundary"

Audited (skip): Plonky3 core (`p3-uni-stark`, `p3-fri`, `p3-merkle-tree`, `p3-symmetric`, `p3-baby-bear`, `p3-poseidon2-air`), Poseidon2-BabyBear-16 round constants (re-audited in-tree at `crates/zkmcu-poseidon-audit`), Blake3 1.8.

In scope:

1. **AIR soundness** (`crates/zkmcu-verifier-plonky3/src/pq_semaphore.rs`):
   - Cross-row constraints: `id_col` continuity, `scope_col` continuity, sibling/prev_digest binding, conditional Merkle swap correctness across all 16 trace rows.
   - Public-input binding: every PI element constrained to its trace cell.
   - Boundary constraints: first-row / last-row constraints close the chain.
   - DIGEST_WIDTH=6 column count derivation matches the AIR layout.
   - Phase B "capacity-4 audit risk: not real" claim (status board entry, security-128bit-plan.md) re-verified row-by-row.
2. **Wire-format / parser:**
   - postcard length-cap (`MAX_PROOF_SIZE`) rejects oversized proofs *before* allocation, on both legs.
   - Trailing-byte rejection on every parse entry point.
   - Integer-overflow review on length fields (the `usize` cast paths around `len-N` parser indices).
   - Padding / alignment of `[u8] → BabyBear` conversions.
3. **Dual-leg binding** (`pq_semaphore_dual.rs`, `pq_semaphore_blake3.rs`):
   - Both legs absorb identical public inputs into transcript.
   - Both legs commit to identical statement `(merkle_root, nullifier, signal_hash, scope_hash)`.
   - Drop-between pattern actually frees what it claims (no leaked Arc, no static buffer reuse hazard, no dangling reference into the dropped p2 leg).
4. **Phase F constant threading:**
   - 16+17 grinding constants match prover-side ↔ verifier-side. Re-grep for any duplicate constant defined elsewhere with stale value.
   - Both legs (`pq_semaphore.rs:205`, `pq_semaphore_blake3.rs:70`) bumped, not just one.
5. **Side channels in new code:**
   - No `if secret { ... } else { ... }` branches in the verify hot path.
   - No `[secret_index]` array indexing.
   - Any `Result<_, Error>` early-return on secret-dependent data flagged for Phase H review (should be 0 by then; re-confirm).
6. **Phase G fuzz follow-through:**
   - Every Medium / Low / Info finding from Phase G's `fuzz/findings/` reviewed; decide fix-vs-defer for each.
7. **Phase H CT path:**
   - Verify the "static canonical bad proof" placeholder used on parse failure is genuinely input-independent.
   - Confirm the `r1 & r2` collapse uses bitwise `&` not short-circuit `&&` in the actual generated assembly (spot-check `cargo asm`).

### Predictions (soft, but written before review per the methodology)

- 0–1 Critical (something we'd fix today regardless)
- 0–2 High (soundness-relevant, fixed before external audit)
- 3–8 Medium (defensible-but-fix-anyway)
- 5–15 Low / Info (style, defensiveness, optional hardening)

If we find > 1 Critical, that's a model violation about how mature the code is. The external-audit ask gets pushed back until we understand why and the bar for the next self-audit pass goes up.

### Output

`research/notebook/<date>-self-audit-pq-semaphore.md` — bug-bounty-style format, severity buckets, reproduction notes for every finding.

Critical + High get fixed inline before the doc closes. Medium + Low + Info get logged for the external auditor as known starting points. The doc itself is part of the external-audit deliverable: handing the auditor a list of "things we already found and fixed" is a strong signal of code maturity.

### Success criteria

- Every Critical / High closed (fix committed, link from finding to commit hash).
- Every Medium logged with a recommended fix or a documented decision to defer.
- Doc committed to `research/notebook/`. Public.

---

## What this plan does NOT cover

- **External audit ask + funding** (Phase J — separate workstream, sequenced after I closes).
- **Documentation / writeup polish / ePrint conversion.** Deferred per artifact-first sequencing: ship the hardened working repo first, polish the writeup second. The outline at `research/notebook/2026-04-30-writeup-outline.md` is the capture; conversion to long-form / ePrint comes after Phase J.
- **Production-silicon tamper hardening** (signed-boot via OTP fuses, SWD lock, hardware AES, glitch detectors, optional secure element). Tracked separately as the production-track plan; not relevant on the dev unit.
- **Power / EM / fault analysis.** Lab equipment required.
- **MPC / multi-device split.** Architectural pivot, separate plan.
- **Instruction-level constant-time review of vendored Plonky3 / BabyBear / Poseidon2 code.** Out of scope; macro-scale CT is the current bar.

---

## Status board (update as you go)

```
Phase G — parser fuzz coverage      : not started
Phase H — constant-time verify      : not started
Phase I — self-audit pass           : not started
```

When a phase lands, append the result line in place (matches the convention from `2026-04-29-security-128bit-plan.md`):

```
Phase G — parser fuzz coverage      : landed YYYY-MM-DD
                                       <one line per target with 1-hour campaign result>
                                       <bug-count tally vs prediction>
                                       runs: fuzz/findings/<date>-<slug>/  (if any)
                                       artifacts: fuzz/fuzz_targets/pq_semaphore_*.rs
```

---

## Quick reference for a fresh session

If you're picking this up cold: read `research/notebook/2026-04-29-security-128bit-plan.md` § Status board first to understand the post-Phase-F state, then `CLAUDE.md` for Rust conventions and the manual flash workflow, then this file. The benchmark schema is at `benchmarks/schema.md`. The mutation pattern set is at `crates/zkmcu-vectors/src/mutations.rs`.

Phase G is pure host-side work (no flashes). Phase H needs the manual Pi 5 flash loop twice per ISA. Phase I is a structured read of code already on disk; no flashes, no compute.
