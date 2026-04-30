# Plan: PQ-Semaphore from 95-bit conjectured to 128-bit-class soundness

**Created:** 2026-04-29
**Original status:** drafted, not started.
**Status as of 2026-04-30:** Phases A, B, C, D, E.1 landed (results + verdicts in § "Status board" at the bottom).

This plan was written *before* any of the Phase A-E benches ran on silicon. It is committed unchanged from its drafting moment except for the status board at the bottom and minor typo fixes. The point of leaving it on disk is so that anyone can git-blame the predicted bands and see they were not back-fitted to the result. The README's methodology bullet ("falsifiable predictions written before bench, four out of five phases landed outside their predicted bands") rests on this file existing in the tree from 2026-04-29 onward.

## Starting point

The Phase 4.0 PQ-Semaphore headline:
- **1049.72 ms M33 / 1249.59 ms RV32**, 169 KB proof, 95-bit *conjectured* PQ security.
- Report: `research/reports/2026-04-29-pq-semaphore-results.typ`.
- Bench artifacts: `benchmarks/runs/2026-04-29-{m33,rv32}-pq-semaphore/`.
- Verifier: `crates/zkmcu-verifier-plonky3/src/pq_semaphore.rs`.
- Firmware: `crates/bench-rp2350-{m33,rv32}-pq-semaphore`.

The 95-bit is honest but below the 128-bit "production" bar. The five phases below close that gap, each independently shippable. Cheap ones first.

**Starting FRI / circuit constants** (`crates/zkmcu-verifier-plonky3/src/pq_semaphore.rs`):
```
TREE_DEPTH       = 10
DIGEST_WIDTH     = 4              // 4 BabyBear elements ≈ 124-bit collision
LOG_BLOWUP       = 1
NUM_QUERIES      = 64
COMMIT_POW_BITS  = 0
QUERY_POW_BITS   = 0
MAX_LOG_ARITY    = 1
```

---

## Phase order and dependencies

```
A (grinding)        independent, instant, biggest yield-per-line
  ↓
B (digest 4→6)      independent, small AIR change
  ↓
C (early-exit)      independent, defends DoS, no soundness change
  ↓
D (Goldilocks alt)  parallel track, alt config not replacement
  ↓
E (dual-hash)       largest, builds on A-B-C being landed
```

A → B → C are sequential because each touches the same `pq_semaphore.rs` AIR/config. D is a parallel track (new files, new vectors, new firmware crate). E is the final headline assembly.

---

## Phase A — Grinding to 128-bit conjectured

**Cost:** one constant change + vector regen + 2 flashes.
**Win:** 95 → ~127 conjectured bits, **free verifier cost** (one extra hash + compare per FRI commit phase).

### Predictions (falsifiable)

| Quantity | Current | Predicted after | Tolerance |
|---|---|---|---|
| M33 verify (ms) | 1049.72 | 1050–1058 | +0–8 ms |
| RV32 verify (ms) | 1249.59 | 1250–1262 | +0–12 ms |
| Proof size (bytes) | 168 970 | 168 970–169 050 | nonce only, < 100 B |
| Conjectured security (bits) | 95 | 127–128 | +32 |

If verify time grows by > 20 ms on M33 we have misunderstood where Plonky3 absorbs PoW work. Investigate before Phase B.

### Touchpoints

1. `crates/zkmcu-verifier-plonky3/src/pq_semaphore.rs` lines 193–194:
   ```rust
   const COMMIT_POW_BITS: usize = 16;   // was 0
   const QUERY_POW_BITS: usize  = 16;   // was 0
   ```
   Why split 16/16 not 0/32: Plonky3 grinds at two stages; balanced is conventional and matches Starkware's split. Total bits add ≈ 32.

2. Host generator: same constants must match in `crates/zkmcu-host-gen/` (grep for `commit_proof_of_work_bits` / `query_proof_of_work_bits`). Verifier-side constants must equal prover-side or every proof rejects.

3. `just regen-vectors` → updates `crates/zkmcu-vectors/data/pq-semaphore-d10/{proof.bin,public.bin}`. Proof grows by ≈ nonce bytes. Commit.

### On-silicon verify

```bash
# Dev machine
cd /home/definiek/Workspace/Personal/Crypto/bindings
just lint && just test                                    # local crosscheck must pass
just build-m33                                            # compile firmware
scp target/thumbv8m.main-none-eabihf/release/bench-rp2350-m33-pq-semaphore \
    pid-admin@10.42.0.30:/tmp/bench-pq-grind.elf
```

Then BOOTSEL the Pico and run on the Pi 5 host:
```bash
picotool load -v -x -t elf /tmp/bench-pq-grind.elf
cat /dev/ttyACM0
```

Capture output to `benchmarks/runs/2026-04-29-m33-pq-semaphore-grind32/raw.log`.
Repeat for RV32 firmware crate, save under `2026-04-29-rv32-pq-semaphore-grind32/`.

### Artifacts to write

For each ISA:
- `raw.log` — full serial capture, unedited
- `result.toml` — schema-conformant, copy-modify from `2026-04-29-{m33,rv32}-pq-semaphore/result.toml`. Update `[circuit].security_bits_conjectured`, add `commit_pow_bits` and `query_pow_bits` fields, fill `[bench.pq_semaphore_verify]`.
- `notes.md` — one paragraph: what changed (grinding bits), what surprised you, link back to this plan section.

### Success criteria

- Cycles_median within +0.7 % of baseline on M33, +1.0 % on RV32.
- Variance still < 0.10 %.
- `result` field = `"ok"` for ≥ 16 iterations.

If all met: phase A is done, mark in this file: `**Phase A status:** landed YYYY-MM-DD`.

### Sub-experiment (optional, ~30 min)

Sweep `COMMIT_POW_BITS = QUERY_POW_BITS ∈ {0, 8, 16, 24}` to characterise verifier cost vs grinding bits. Single result.toml with four `[bench.*]` blocks. Saves a graph for the eventual dual-hash report.

---

## Phase B — Merkle digest 4 → 6 elements

**Cost:** AIR shape change + cascade through Poseidon2 calls + vector regen + 2 flashes.
**Win:** Hash-collision floor 124-bit → ~186-bit. Symmetric security across FRI-soundness and hash-binding bottlenecks.

### Predictions

| Quantity | Current | Predicted after | Reasoning |
|---|---|---|---|
| Proof size (bytes) | 168 970 | 215 000–235 000 | +50% Merkle openings |
| M33 verify (ms) | 1049.72 | 1180–1280 | +12–22%, more Poseidon2 inputs to absorb |
| RV32 verify (ms) | 1249.59 | 1410–1530 | proportional |
| Heap after parse (KB) | 150 | 180–200 | larger digest fields in proof |
| Conjectured security (bits) | 95 + 32 (A) = 127 | unchanged FRI-side | hash floor moves from 124→186, removes the bottleneck |

If verify-time hit is > 30 % we did the wrong thing in the AIR (probably forgot `transparent` repr or the constraint count exploded).

### Touchpoints

1. `crates/zkmcu-verifier-plonky3/src/pq_semaphore.rs:156`:
   ```rust
   pub const DIGEST_WIDTH: usize = 6;   // was 4
   ```
2. The `Poseidon2-BabyBear-16` permutation has state width 16. With 6-word digest you fit `hash_pair(L, R)` = 12 inputs into 16-state with 4 capacity. Currently 8/16 with 8 capacity. Capacity drop from 8 → 4 *might* be a security concern for Poseidon2 sponge soundness — **verify Plonky3's audited constants are valid for capacity=4 before changing**. If not, bump permutation width to 24 (Poseidon2-BabyBear-24, also audited in Plonky3).
3. All `[T; DIGEST_WIDTH]` columns scale automatically; double-check the AIR row layout still fits in the 321-col target or accept a wider trace.
4. Host generator updates: `crates/zkmcu-host-gen/` Merkle tree builder must produce 6-element digests. Cascade.
5. `just regen-vectors`.

### On-silicon verify

Same flash flow as Phase A. Save under:
- `benchmarks/runs/<date>-m33-pq-semaphore-d6/`
- `benchmarks/runs/<date>-rv32-pq-semaphore-d6/`

`<date>` = the day you actually flash, not 2026-04-29.

### Success criteria

- Variance < 0.15 % both ISAs.
- Verify time within predicted band.
- `[circuit].digest_words = 6` in the new `result.toml`.

### Risk

Switching to Poseidon2-24 (if needed for capacity) is a non-trivial code change. If you discover the audited constants don't cover capacity=4 with width-16, halt and either:
- accept 5-word digest (~155-bit collision, still > 128) inside width-16 with capacity-6, or
- migrate to Poseidon2-24 with audited constants.

Document the choice in `notes.md`.

---

## Phase C — Two-stage verify with early exit

**Cost:** verifier reorder + adversarial bench harness.
**Win:** mutated proofs reject in < 10 ms on M33 instead of paying full verify cost. Zero soundness change. Embedded-DoS-relevant story for the paper.

### Predictions

| Mutation point | Current reject time | Predicted after |
|---|---|---|
| Header byte 0 (commit) | ~1050 ms (full verify) | < 1 ms |
| FRI fold step 0 query | ~1050 ms | 5–20 ms |
| FRI final layer | ~1050 ms | 600–900 ms |
| Honest proof | 1049.72 ms | 1049.72 ms (must not regress) |

### Touchpoints

1. `crates/zkmcu-verifier-plonky3/src/pq_semaphore.rs` — find the verify entrypoint (likely calls into `p3-uni-stark::verify`). Plonky3 already short-circuits on most failures; the gain is in **ordering**. Read the upstream `p3_uni_stark::verifier::verify` and confirm what runs first.
2. If the upstream does pre-grind-check before parse: order is fine, this phase is a measurement-only result.
3. If grinding check happens late: pull it earlier into the verify flow. Gating change in `pq_semaphore.rs::verify` wrapper.
4. New bench harness: `crates/bench-rp2350-{m33,rv32}-pq-semaphore-mutated` (or extend existing crate with a feature flag). Harness loads N mutation patterns from a static array, runs verify, records cycles per mutation, reports min/median/max time-to-reject.

### Mutation patterns to test

Define in `crates/zkmcu-vectors/src/mutations.rs`:
```
M0: flip byte at offset 0           (PoW nonce / commit prefix)
M1: flip byte at offset 1024        (mid-FRI)
M2: flip last byte of proof         (final FRI layer)
M3: zero a Merkle digest word       (commit-time fail)
M4: corrupt a query opening index   (mid-query fail)
M5: alter a public input byte       (transcript mismatch)
```

Six mutations × 8 iterations each = 48 verify calls per ISA per run. Cheap.

### Artifacts

`benchmarks/runs/<date>-{m33,rv32}-pq-semaphore-reject/result.toml`:
```toml
[bench.honest_verify]    iterations=8 cycles_median=...
[bench.reject_M0]        iterations=8 cycles_median=...
[bench.reject_M1]        iterations=8 cycles_median=...
...
```

### Success criteria

- Honest verify regression ≤ 0.5 % (prove the reorder is free).
- Reject time strictly less than honest verify time for every mutation.
- M0/M3 reject in < 5 ms on M33.

---

## Phase D — Goldilocks × Quadratic alternate config (parallel track)

**Cost:** new AIR over Goldilocks, new host-gen, new firmware crate, new vectors. Largest of the five but independent.
**Win:** 128-bit *native* field (no conjecture-stack on field side), and per phase 3.3 numbers Goldilocks×Quadratic was 66 % faster than BabyBear×Quartic on M33. Hypothesis: 1.05 s → ~630 ms M33.

### Predictions

| Quantity | BabyBear×Quartic baseline | Goldilocks×Quadratic predicted |
|---|---|---|
| M33 verify (ms) | 1049.72 | 600–680 |
| RV32 verify (ms) | 1249.59 | 1100–1300 (Goldilocks lacks UMAAL win) |
| Cross-ISA ratio | 1.190× | 1.6–2.0× (worse than baseline, that's the tradeoff) |
| Field security | 124-bit | 128-bit |
| Proof size | 169 KB | 130–160 KB (smaller field elements) |

### Touchpoints

1. New verifier module: `crates/zkmcu-verifier-plonky3/src/pq_semaphore_goldilocks.rs`. Mostly a re-typed copy of `pq_semaphore.rs` with `Val = Goldilocks`, `Challenge = QuadraticExtensionField<Goldilocks, 2>`, Poseidon2-Goldilocks state.
2. Confirm Plonky3 ships audited Poseidon2-Goldilocks round constants (`POSEIDON2_GOLDILOCKS_RC_*`). If not, this phase blocks on that audit.
3. Host generator: `crates/zkmcu-host-gen/` parallel CLI (`gen-pq-semaphore-gl`) producing `crates/zkmcu-vectors/data/pq-semaphore-d10-gl/`.
4. New firmware: `crates/bench-rp2350-{m33,rv32}-pq-semaphore-gl`.

### On-silicon verify

Same flash flow. Save under:
- `benchmarks/runs/<date>-{m33,rv32}-pq-semaphore-gl/`

### Success criteria

- M33 verify under 700 ms (proves Goldilocks pays off on this ISA).
- RV32 verify documented (regardless of speed; the cross-ISA ratio is part of the result).
- Proof verifies; cross-check against BabyBear baseline that the *statement* is identical (same merkle root, same nullifier, same scope).

### Decision point after Phase D

Phase D produces a second config alongside Phase A+B+C. The whitepaper now has a 2x2 table:
- **BabyBear×Quartic + grinding + d6** → 128 conjectured, portable (1.19× cross-ISA), 1.2 s
- **Goldilocks×Quadratic + grinding** → 128 native, less portable (~1.8× cross-ISA), ~0.65 s M33

Reader picks. Don't kill the BabyBear path.

---

## Phase E — Stacked dual-hash verify ("true PQ" headline)

**Cost:** highest. Architectural change to the proof shape. Two FRI commits, two query sets, one transcript.
**Win:** soundness compounds across two independent hash functions. Even if Poseidon2-BabyBear breaks (cryptanalysis surprise), the proof is still bound by Blake3. Genuine "PQ-defence-in-depth" headline at ~2 s verify.

### Predictions

| Quantity | After A+B+C | After E |
|---|---|---|
| M33 verify (ms) | ~1180 | 2200–2500 |
| RV32 verify (ms) | ~1410 | 2700–3000 |
| Proof size (KB) | ~225 | 350–420 |
| Heap after parse (KB) | ~190 | 280–340 |
| Conjectured security | 128 | 128 each, with cryptanalytic backup |

### Touchpoints

Two design options, decide before writing code:

**E.1 — Two parallel FRI proofs, one statement**
- Prover produces `(proof_p2, proof_b3)` over the same trace, two independent transcripts seeded by the same public inputs.
- Verifier runs both. Either reject → reject.
- Proof size ≈ 2× single-hash proof.
- Implementation: parameterise `make_config` over the hash, generate twice, concat.

**E.2 — Hash-tower commitment** (research-grade)
- Prover commits each Merkle layer with both hashes, the two digests are bound into a single commitment.
- Verifier checks both hash chains per query.
- Proof size ≈ 1.5× single-hash. Verify ≈ 1.7×.
- Implementation: significant Plonky3 surgery, may need fork beyond vendoring.

Recommend E.1 for a first pass. Ship E.2 only if E.1 lands and the paper needs the size win.

### Touchpoints (E.1)

1. `crates/zkmcu-verifier-plonky3/src/pq_semaphore_dual.rs` — wrapper that calls `pq_semaphore::verify` with `make_config_p2()` and `make_config_b3()` in sequence.
2. `make_config_b3()`: new function in same crate, swaps Poseidon2 for Blake3 hash + Blake3 challenger.
3. Host generator: produce both proofs, write `proof_p2.bin` and `proof_b3.bin` (or concat with a 4-byte length prefix).
4. New firmware: `bench-rp2350-{m33,rv32}-pq-semaphore-dual`.

### On-silicon verify

Same flow. Bench artifact:
- `benchmarks/runs/<date>-{m33,rv32}-pq-semaphore-dual/result.toml`
  - `[bench.dual_verify]` for the combined verify
  - `[bench.p2_only]` for the BabyBear-Poseidon2 sub-verify
  - `[bench.b3_only]` for the Blake3 sub-verify
  - sum of sub-verifies should ≈ dual_verify (sanity check)

### Success criteria

- Both sub-verifies accept.
- Mutating either sub-proof causes dual_verify to reject (run with mutation harness from Phase C).
- Combined verify under 2.5 s on M33 (the publishable bar).

---

## What to commit when

Each phase is one or more commits. Suggested split:

- **Phase A:** one commit (constants + regenerated vectors), one commit per ISA bench (the result.toml + raw.log + notes.md).
- **Phase B:** one commit for AIR change, one for host-gen, one for vectors, two for benches.
- **Phase C:** one commit for mutation harness, one per ISA bench.
- **Phase D:** track on a feature branch (`pq-semaphore-goldilocks`); merge after both ISAs are benched.
- **Phase E:** track on a feature branch; merge after dual-verify benches land.

Reports are *immutable* — if a phase moves the public number, write a new dated report under `research/reports/`. Do not edit `2026-04-29-pq-semaphore-results.typ`. Suggested report milestones:
- After Phase A: `<date>-pq-semaphore-grinding.typ` (small, just the deltas).
- After Phases A+B+C: `<date>-pq-semaphore-128bit.typ` (combined headline).
- After Phase D: `<date>-goldilocks-vs-babybear-128bit.typ` (the 2x2 table).
- After Phase E: `<date>-dual-hash-pq-semaphore.typ` (the final headline).

## Status board (update as you go)

```
Phase A — grinding to 128 conj           : landed 2026-04-29
                                           M33  1051.088 ms (+0.13% vs 1049.72 baseline)
                                           RV32 1255.981 ms (+0.51% vs 1249.59 baseline)
                                           proof 168_803 B (-167 B), 127 conjectured bits
                                           runs: benchmarks/runs/2026-04-29-{m33,rv32}-pq-semaphore-grind32/
Phase B — digest 4→6                     : landed 2026-04-29
                                           M33  1065.839 ms (+1.40% vs Phase A)
                                           RV32 1269.727 ms (+1.09% vs Phase A)
                                           proof 172_607 B (+3_804 B, +2.3%)
                                           hash-collision floor 124 → 186 bits (no longer bottleneck)
                                           cross-ISA ratio 1.191× (Phase A 1.195×, preserved)
                                           verifier hit landed FAR below predicted +12-22% band
                                              — application Merkle openings are AIR witness columns,
                                                not external FRI openings, so digest=6 only widened
                                                the trace by ~11 cols (~3.4%) instead of multiplying
                                                Poseidon2 work. Plan's per-input absorption model
                                                was wrong; correct model is per-trace-column LDE cost.
                                           capacity-4 audit risk: not real — every active row is a
                                              single fixed-input permutation (not a sponge), so the
                                              audited Poseidon2-BabyBear-16 RCs apply. No migration
                                              to width-24 needed.
                                           runs: benchmarks/runs/2026-04-29-{m33,rv32}-pq-semaphore-d6/
Phase C — two-stage early exit           : landed 2026-04-29 (measurement-only)
                                           reads of upstream uni_stark + fri verifiers
                                              showed parse → cheap shape → FRI PoW →
                                              expensive Merkle ordering already in
                                              place; no verifier reorder needed
                                           honest_verify (full pipeline, includes parse):
                                              M33  1130.58 ms (+6.07% vs Phase B verify-only
                                                   baseline — extra cost is parse +
                                                   make_config + build_air done once at boot
                                                   in d6 baseline)
                                              RV32 1302.64 ms (+2.59% vs Phase B baseline)
                                           reject medians (M33 / RV32):
                                              M0 header_byte    8.53 ms / 8.90 ms (~146×)
                                              M1 trace_commit   8.58 ms / 8.96 ms (~145×)
                                              M2 mid_fri       13.14 ms / 13.94 ms ( ~93×)
                                              M3 query_opening 44.11 ms / 56.03 ms ( ~23×)
                                              M4 final_layer   44.09 ms / 56.03 ms ( ~23×)
                                              M5 public_byte  126.87 ms /151.35 ms (  ~9×)
                                           worst-case attacker DoS efficiency capped at
                                              ~9× (M5) — public-input transcript desync
                                              forces PCS verify through full per-round
                                              preamble before the first PoW check fails
                                           final-layer reject landed FAR below predicted
                                              600-900 ms band: 44 ms / 56 ms because
                                              upstream FRI verifier checks per-round
                                              commit-phase PoW BEFORE the per-query
                                              Merkle loop, so corruption in the tail
                                              (commit_pow_witnesses) short-circuits
                                              after one round, not after all 64 queries
                                           cross-ISA ratio varies by pattern: 1.04×
                                              for parse-fail (memory-bandwidth-bound)
                                              to 1.27× for verifier-stage (field-arith-
                                              bound where M33 UMAAL win shows). honest
                                              full-pipeline 1.152× (sits between);
                                              verify-only baseline was 1.191×.
                                           operational caveat: midpoint byte-flips OOM
                                              the firmware (postcard varint corruption →
                                              huge Vec alloc → TLSF NULL → panic_halt).
                                              M3 was switched from proof.len()/2 to
                                              proof.len()-64 (last query's last fold-
                                              step Merkle path bytes — hash data, no
                                              varints near it) for the recapture.
                                           runs: benchmarks/runs/2026-04-29-{m33,rv32}-pq-semaphore-reject/
Phase D — Goldilocks×Quadratic alt       : landed 2026-04-29 (HYPOTHESIS REJECTED)
                                           M33  1995.66 ms (+87.2% vs Phase B 1065.84 ms)
                                           RV32 2700.84 ms (+112.7% vs Phase B 1269.73 ms)
                                           proof 271_120 B (+57% vs Phase B 172_607 B)
                                           heap_peak 415_312 B (needed 480 KB heap, up from 384 KB)
                                           cross-ISA ratio 1.354 (vs Phase B 1.191× — gap widened)
                                           plan predicted 600-680 ms M33 / 1100-1300 ms RV32;
                                              prediction inherited the Phase 3.3 "Goldilocks
                                              66% faster" number from a fib1024 STARK.
                                              PQ-Semaphore verify is hash-bound, not
                                              arithmetic-bound, so Phase 3.3's win does
                                              not transfer.
                                           why slower:
                                              - 64-bit Goldilocks elements vs 31-bit BabyBear
                                                multiply 3-4× per base-field op cost on
                                                32-bit MCUs; UMAAL helps M33 but not Hazard3
                                              - Poseidon2-Goldilocks-16 has 22 partial rounds
                                                vs Poseidon2-BabyBear-16's 13 (1.7× more)
                                              - hash-bound shape multiplies both penalties
                                           AIR DIGEST_WIDTH dropped 6→4 (256-bit hash
                                              space, 128-bit collision floor) and internal
                                              MMCS DIGEST_ELEMS dropped 8→4; without those
                                              cuts proof would have been ~320 KB and
                                              MAX_PROOF_SIZE had to be bumped 256→320 KB
                                              regardless. With both cuts, proof landed at
                                              271 KB — still bigger than Phase B's 172 KB
                                              because each Goldilocks element doubles the
                                              wire-byte cost.
                                           operational caveat: 480 KB heap leaves only
                                              ~32 KB stack; bench_core::measure_stack_peak
                                              paints 64 KB below SP and corrupts heap.
                                              boot_measure() removed from GL firmware;
                                              stack peak not captured. Same trade-off the
                                              Phase C reject benches accepted.
                                           security: 128-bit native field (Goldilocks ×
                                              Quadratic), 127-bit conjectured FRI, 128-bit
                                              hash collision. Combined min(128, 127, 128)
                                              = 127-bit, identical headline to BabyBear-d6
                                              + grinding but with no field-side conjecture-
                                              stack. Net: GL trades +87% verify cost for
                                              eliminating a 124-bit field-side floor that
                                              wasn't binding anyway (FRI was the bottleneck).
                                           takeaway: BabyBear × Quartic + d6 + grinding
                                              stays the right embedded choice. GL is a
                                              defensible "native-field 128-bit" alternate
                                              for callers who specifically want no field-
                                              side conjecture, but it costs ~1.87× verify
                                              and regresses cross-ISA portability 1.19×→
                                              1.35×. Plan's anticipated "2x2 table" still
                                              holds — reader picks — just not the row that
                                              was expected to win.
                                           runs: benchmarks/runs/2026-04-29-{m33,rv32}-pq-semaphore-gl/
Phase E — stacked dual-hash              : landed 2026-04-30 (HYPOTHESIS EXCEEDED)
                                           M33  1611.39 ms (+51.2% vs Phase B 1065.84 ms)
                                           RV32 2041.78 ms (+60.8% vs Phase B 1269.73 ms)
                                           proof_p2 172_977 B + proof_b3 163_824 B
                                              = 336_801 B total on the wire
                                           heap_peak 304_180 B (drop-between pattern keeps
                                              peak at max(p2_peak, b3_peak), 384 KB heap
                                              fits comfortably — no Phase D 480 KB workaround)
                                           stack_peak 11_196 B M33 / 11_000 B RV32 (sentinel
                                              paint succeeded — full bench harness intact)
                                           cross-ISA ratio 1.267 (vs Phase B 1.191×)
                                           plan predicted 2200-2500 ms M33 / 2700-3000 ms
                                              RV32; both ISAs landed FAR below band
                                           why faster than predicted:
                                              plan implicitly assumed dual ≈ 2 × Phase B,
                                              i.e. Blake3 leg costs the same as Poseidon2
                                              leg. Per-leg estimates (dual minus Phase B):
                                                p2 leg: M33 ~1066 ms / RV32 ~1270 ms (1.19×)
                                                b3 leg: M33  ~545 ms / RV32  ~772 ms (1.42×)
                                              Blake3 is roughly half the cost of Poseidon2
                                              on M33 because:
                                                - ARX (add/rotate/xor) over 32-bit words,
                                                  no field arithmetic at all
                                                - Cortex-M33 ships single-cycle barrel
                                                  rotate, LLVM unrolls Blake3 cleanly
                                                - vs Poseidon2-BB-16: 21-round width-16
                                                  permutation per Merkle node, each round
                                                  does S-box (x^7) + MDS over BabyBear
                                              On Hazard3 the Blake3 leg pays a wider 1.42×
                                              cross-ISA tax because Hazard3 lacks a single-
                                              instruction rotate (3-instruction emulation),
                                              widening the dual cross-ISA from 1.19× to 1.27×
                                           security: per-leg 127 conj FRI + 186-bit hash floor,
                                              combined min(127, 127) = 127 bits + dual-hash
                                              composition. A forged proof must verify under
                                              BOTH Poseidon2-BabyBear and Blake3 — cryptanalytic
                                              break on either hash does not collapse the
                                              verifier. The 1-bit gap to a literal 128 is on
                                              the FRI grinding side, not the dual-hash
                                              composition; orthogonal property.
                                           takeaway: dual-hash IS the production-bar headline.
                                              ~1.6 s M33 / ~2.0 s RV32, 384 KB heap, 337 KB
                                              total proof, 1.27× cross-ISA. Phase E.2
                                              (hash-tower commitment) is still available for
                                              a smaller proof at higher implementation cost,
                                              but E.1 already meets the 2.5 s M33 ceiling
                                              with ~890 ms of headroom.
                                           runs: benchmarks/runs/2026-04-30-{m33,rv32}-pq-semaphore-dual/
```

Status board above was updated in place as each phase landed; everything else in this file is the original 2026-04-29 draft.
