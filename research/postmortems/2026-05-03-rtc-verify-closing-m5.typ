#import "/research/lib/template.typ": *

#show: paper.with(
  title: "Closing the M5 public-input timing oracle: a run-to-completion verify variant for Plonky3",
  authors: ("zkmcu",),
  date: "2026-05-03",
  kind: "postmortem",
  abstract: [
    First on-silicon ct-reject sweep of the dual-hash CT verifier (`pq_semaphore_dual_ct`, Phase H) on RP2350 Cortex-M33 surfaced a 9.46x wall-clock speedup on `Mutation::M5_public_byte`: 168 ms reject vs 1593 ms honest. M0–M4 (parse-fail and Merkle-fail mutations) all sat within 0.06% of honest. A remote attacker with any wall-clock side-channel could therefore distinguish "public-input mutated" from any other failure class with single-probe certainty. Closed by vendoring Plonky3 and adding parallel `verify_run_to_completion` entry points across `p3-uni-stark`, `p3-fri`, and `p3-commit` that accumulate data-path failures into a single status flag instead of `?`-propagating the first one. Re-run on M33: M5 lands at 0.99977 of honest, statistically tied with M0–M4. Macro-CT restored across the full mutation set.
  ],
)

= What the bench found

`benchmarks/runs/2026-05-03-m33-pq-semaphore-ct-reject/` measured 16 iterations of `verify_constant_time` per pattern in `zkmcu_vectors::mutations::ALL`:

#table(
  columns: 3,
  align: (left, right, right),
  stroke: 0.4pt,
  table.header[Pattern][median (ms)][ratio to honest],
  [honest_verify], [1593.611], [1.00000],
  [reject_M0_header_byte], [1593.554], [0.99996],
  [reject_M1_trace_commit_digest], [1593.677], [1.00004],
  [reject_M2_mid_fri], [1593.516], [0.99994],
  [reject_M3_query_opening], [1593.444], [0.99990],
  [reject_M4_final_layer], [1593.568], [0.99997],
  [*reject_M5_public_byte*], [*168.383*], [*0.10565*],
)

M5 flips bit 0 of `public[0]`. The element stays canonical (less than the BabyBear modulus) so `parse_public_constant_time` accepts. The corrupted byte then desyncs the Fiat-Shamir transcript when `p3-uni-stark::verify_with_preprocessed` calls `challenger.observe_slice(public_values)`. Every challenge sampled afterwards (alpha, zeta, the FRI betas) diverges from the prover's. The first commitment-replay check fails inside the FRI commit-phase Merkle verification, which `?`-propagates back through `verify_fri` and `pcs.verify` to `verify_with_preprocessed`. Cycle count drops from ~239 M to ~25 M.

The pre-existing host parity test (`ct_matches_phase_c_on_every_mutation`) only checked the boolean accept/reject decision on every mutation. Both paths return `false` on M5 — boolean parity holds. Wall-clock was never measured. The leak only surfaced when the on-silicon ct-reject harness ran across the full `ALL` set.

= Why this is a real attack surface

`pq_semaphore_dual_ct` documents itself as macro-CT: total verify time independent of mutation class. The threat model is a remote attacker who can measure response latency through any channel — USB CDC reply timing, network jitter, or a voltage / EM probe on the device. That attacker can already distinguish `honest_accept` from `not-honest_accept`; the macro-CT property says they cannot distinguish *which* failure class within `not-honest_accept` they triggered.

M5 violates that property cleanly. Public inputs in PQ-Semaphore are `merkle_root`, `nullifier`, `signal_hash`, `scope_hash`. None are secret in themselves. But the timing oracle reveals something stronger than any individual public-input bit: it reveals *whether the failure was caused by a public-input flip versus any other class*. That distinguishes attacker-controlled tampering with the user's broadcast from network / proof corruption, which is exploitable in protocols where the verifier's response has different downstream effects depending on the failure mode.

= Remediation menu and choice

Four options, in increasing effort:

+ *Document and ship.* Narrow the macro-CT claim to "parse-fail and Merkle-fail mutation classes only; public-input mutations remain known-leakage pending a Plonky3 verify variant that runs to completion." Cheapest, but leaves a real timing oracle on the wire.
+ *Pad M5's reject path with a dummy verify.* Run a second `verify_with_config` on the static fallback inputs whenever the first returns `Err` post-parse. Half-measure: stacks two verifies' worth of jitter, requires care that the dummy can't itself fast-fail, and doesn't address the layer where the leak lives.
+ *Vendor + patch Plonky3 verify with a "run to completion" mode.* Modify the vendored `p3-uni-stark::verify` and its FRI / Pcs-trait dependencies so that internal failures don't return early. Every commit-phase Merkle check, every PoW witness, every query, the final-poly check all run, and the results AND together at the end. Closes the leak at the layer it lives in.
+ *Threat-model-only fix.* Argue that public inputs aren't secret data, so a timing oracle on "is the public input the right one" doesn't leak any secret. True for some narrowly-scoped threat models, false for the macro-CT claim the module shipped with.

We picked C. Hardware-token security is the explicit goal of zkmcu; A and D would have required walking back a security claim, B is a half-measure with extra jitter, C produces a reusable artifact (a CT-safe Plonky3 verify variant) useful beyond zkmcu. The fork-maintenance cost is real but accepted.

= Patch surface

Three vendored crates touched. The original `verify` / `verify_with_preprocessed` / `verify_fri` paths are unchanged; the new entry points sit alongside them so non-CT callers stay on the fast path.

== `vendor/Plonky3/uni-stark/src/verifier.rs`

- `pub fn verify_run_to_completion` — same signature as `verify`, delegates to the next.
- `pub fn verify_with_preprocessed_run_to_completion` — identical body to `verify_with_preprocessed` except the `pcs.verify(...)?` call becomes `pcs.verify_run_to_completion(...).map_err(...)` captured into a local result, the `verify_constraints(...)?` call becomes a captured result, and the function ends with a `match` over both. First failure wins for diagnostics; both must hold for accept.

== `vendor/Plonky3/commit/src/pcs/univariate.rs`

- New `Pcs::verify_run_to_completion` trait method, default implementation delegating to `Pcs::verify`. Non-CT PCS implementations need no changes; CT-aware implementations override.

== `vendor/Plonky3/fri/src/two_adic_pcs.rs`

- Override `Pcs::verify_run_to_completion` for `TwoAdicFriPcs`. Same transcript-observation prologue as `verify`, then dispatches to `verifier::verify_fri_run_to_completion`.

== `vendor/Plonky3/fri/src/verifier.rs`

- New `verify_fri_run_to_completion` parallel to `verify_fri`. Maintains an `Option<FriError>` accumulator (first failure recorded; subsequent ones merged in). Every prover-controlled `?` becomes a record-and-continue.
- `verify_query_run_to_completion` — commit-phase Merkle batch failures recorded; folding continues with the prover-supplied evaluations (which were already reconstructed from `folded_eval` plus `opening.sibling_values`, independent of the Merkle outcome).
- `open_input_run_to_completion` — input-MMCS Merkle batch failures recorded; reduced-opening computation continues with the prover-supplied opened values. The constant-degree `log_blowup`-height check at the end of each batch becomes a record-and-continue.
- The two `check_witness` calls (per-round and query-phase PoW) are run for their challenger-state side effect regardless of result; failure is recorded and the subsequent `sample_algebra_element` returns the same `beta` as on the honest path.

== Scoping note: shape errors keep their early-returns

The variant is CT against *valid-shape proofs with arbitrary content*. Shape errors — count mismatches, dimension mismatches, range / height / arity mismatches — keep their early-returns because:

+ They are data-independent. A mutated proof byte cannot change a `Vec::len()`. For any honestly-produced proof byte-mutated downstream, no shape error fires; it would have to fire on the unmutated proof too.
+ Continuing past them is unsafe. The downstream code uses `zip` / indexing on collections whose lengths these checks validate. Continuing past a length mismatch produces iterator panics, not bad answers.
+ Shape validation already happens upstream. Inputs reach this layer through the postcard-based `parse_*_constant_time` helpers, which reject shape errors before any verify code runs.

Documented inline in both `verify_run_to_completion` and `verify_fri_run_to_completion`.

== zkmcu-side wiring

- `crates/zkmcu-verifier-plonky3/src/pq_semaphore.rs::verify_with_config_rtc` — calls `p3_uni_stark::verify_run_to_completion`.
- `crates/zkmcu-verifier-plonky3/src/pq_semaphore_blake3.rs::verify_with_config_rtc` — sibling for the Blake3 leg.
- `crates/zkmcu-verifier-plonky3/src/pq_semaphore_dual_ct.rs` — `verify_p2_leg_constant_time` and `verify_b3_leg_constant_time` switched to dispatch through the RTC variants.
- `crates/zkmcu-verifier-plonky3/tests/pq_semaphore_dual_ct_timing.rs` — new host-side wall-clock parity test using `Instant::now()`. For every mutation in `zkmcu_vectors::mutations::ALL`, asserts the median of three runs lands within ±3x of the honest baseline. The ±3x tolerance is intentionally generous so noisy CI doesn't trip it; the on-silicon ct-reject bench remains the precision instrument.

= Verification

== Host

- All six pre-existing `pq_semaphore_dual_ct` tests still pass — boolean correctness preserved.
- New timing parity test was sanity-checked by reverting both legs to the non-RTC `verify_with_config`. Result: M5 ratio 0.1504, test correctly panics. Reverting to RTC: M5 ratio 0.9862, all mutations within tolerance, test passes.
- `just check` clean across the workspace (rustfmt, clippy with `-D warnings`, all tests).

== On-silicon

`benchmarks/runs/2026-05-03-m33-pq-semaphore-ct-reject-rtc/`, RP2350 Cortex-M33 @ 150 MHz, 16 iterations per pattern. (Iters 2–5 of `honest_verify` scrolled past serial capture; honest stats from $n=12$ for cycles, $n=11$ for µs. Within-pattern variance is ~0.05%, cross-pattern variance ~0.025% — well below typical on-silicon noise floors.)

#table(
  columns: 3,
  align: (left, right, right),
  stroke: 0.4pt,
  table.header[Pattern][median (ms)][ratio to honest],
  [honest_verify], [1590.298], [1.00000],
  [reject_M0_header_byte], [1590.234], [0.99996],
  [reject_M1_trace_commit_digest], [1590.245], [0.99997],
  [reject_M2_mid_fri], [1590.199], [0.99994],
  [reject_M3_query_opening], [1590.205], [0.99994],
  [reject_M4_final_layer], [1590.166], [0.99992],
  [*reject_M5_public_byte*], [*1589.937*], [*0.99977*],
)

M5 went from 168.383 ms (0.10565 ratio, 9.46x leak) to 1589.937 ms (0.99977 ratio, 0.023% under honest). Statistically tied with M0–M4. Wall-clock indistinguishability restored across the full mutation set.

= Open follow-ups

+ *Constraint folder CT.* `verify_constraints` runs the same arithmetic regardless of pass/fail at the high level, but a dedicated audit of the AIR's `eval` impl — particularly the conditional swap and the equality checks — would close the assumption that constraint evaluation is itself instruction-level CT.
+ *MMCS verify_batch CT.* The macro-CT property here relies on Merkle path verification taking constant cycles regardless of input. Plonky3's `MerkleTreeMmcs` appears to be CT (no early returns, no input-dependent branches in the hot path), but a dedicated micro-bench would close that gap formally.
+ *Instruction-level CT.* Macro-CT closes wall-clock leaks. Cache-timing and EM side-channel work remains future scope.
+ *Upstreaming.* The vendored patches are additive — new entry points, no behavioural change to existing surface. Worth proposing to `Plonky3/Plonky3` once the artifact stabilises and the broader STARK-on-MCU community has had a chance to use it.

= Session-level takeaway

Boolean parity tests find the wrong bug class for CT properties. The pre-existing `ct_matches_phase_c_on_every_mutation` test passed cleanly through the entire Phase H development cycle while a 9.46x timing oracle sat in the verifier. The on-silicon ct-reject bench surfaced it on first run because the harness measures cycles per mutation; the host test never did. Lesson: any code path claimed to be CT needs a *timing* assertion, not just a boolean one. The new `pq_semaphore_dual_ct_timing.rs` closes that gap on host.
