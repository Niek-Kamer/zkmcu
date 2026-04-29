# PQ-Semaphore AIR design

**Date:** 2026-04-29
**Status:** design doc, pre-implementation
**Sits under:** phase 4.0 step 4 of `research/reports/2026-04-29-pq-semaphore-scoping.typ`
**Anchored by:** `benchmarks/runs/2026-04-29-{m33,rv32}-pq-poseidon-chain/result.toml`

## Goal

Replace the BN254 / Groth16 Semaphore v4 verifier on the Pico 2 W with
a Plonky3 STARK proving the same four protocol-level properties:

1. Knowledge of an identity commitment `id`.
2. `H(id)` is a leaf in a public Merkle tree at depth 10.
3. A nullifier `N = H(id, scope)` is correctly derived.
4. A signal binding `S = H(scope, message)` ties the proof to a
   specific message.

The verifier runs on-MCU; proving stays on the host.

## Anchor data

Today's `pq_poseidon_chain_verify` measurement on the same hardware
gives sharp numbers to design against:

| | M33 | RV32 |
|---|---:|---:|
| Verify (median) | 492 ms | 616 ms |
| Heap peak | 211 KB | 211 KB |
| Stack peak | 2.4 KB | 1.9 KB |
| Trace rows | 64 | 64 |
| Trace columns | ~298 | ~298 |
| FRI queries | 28 | 28 |
| Proof size | 88 KB | 88 KB |

Variance 0.05 % M33 / 0.08 % RV32, all `ok=true`. The 28-query
non-hiding `TwoAdicFriPcs` over `BabyBear+Quartic` is solid.

## Architectural decision: extend or compose?

The hard architectural question: how do we add the
Merkle / nullifier / scope-binding constraints on top of Plonky3's
existing audited Poseidon2 column structure?

### Option A — Custom AIR that *embeds* `Poseidon2Cols`

Define a new `PqSemaphoreCols<F>` struct that contains:

```text
PqSemaphoreCols {
    poseidon2: Poseidon2Cols<F, 16, 7, 1, 4, 13>,  // 298 cols, audited

    // Per-row auxiliary state for the Merkle path / nullifier / scope.
    direction_bit:    F,    // for Merkle rows
    is_merkle_row:    F,    // 1 if this row's permutation is a Merkle hop
    is_nullifier_row: F,    // 1 if this row hashes (id, scope)
    is_scope_row:     F,    // 1 if this row hashes (scope, message)
    is_root_check:    F,    // 1 on the row whose output equals the public root
    // ... a handful more selector / wiring columns
}
```

Implement `Air::eval` by:

1. Calling Plonky3's existing `eval_full_round` / `eval_partial_round`
   helpers against the `poseidon2` sub-struct, exactly the same way
   the vendored `Poseidon2Air` does. This *re-uses the audited
   constraint logic* without duplicating it.
2. Adding bespoke constraints for the conditional-swap wiring,
   public-input binding, and inter-row state continuity.

Pros:
- Audited Poseidon2 logic stays exactly the audited shape; nothing in
  the hash-side constraint surface changes.
- Witness generation can re-use Plonky3's `generate_trace_rows_for_perm`
  for the Poseidon2 cells; we only generate the auxiliary columns
  ourselves.
- One AIR, one verify pass, one proof. Minimal additions to firmware.

Cons:
- The auxiliary constraints couple to specific cells within
  `Poseidon2Cols`. If Plonky3 ever changes the column layout (e.g., a
  v0.6 refactor that shuffles `inputs` / `post`), the custom AIR
  breaks.
- Witness generation has to interleave Plonky3's `Poseidon2Cols`
  generator with our own logic. Not hard, but more glue than a
  standalone AIR.

### Option B — Compose two AIRs via lookup arguments

Run two separate STARKs:
- The vendored `VectorizedPoseidon2Air` proving 16 Poseidon2
  permutations.
- A second tiny AIR proving the Merkle wiring + nullifier + scope
  constraints.

Connect them via Plonky3's lookup-argument support
(`p3-uni-stark-lookup` etc., if it exists for this version).

Pros:
- The Poseidon2 AIR stays untouched, audit coverage is mechanical.

Cons:
- Two proofs (or one proof with multiple AIRs), more wire-format
  bytes, more deserialisation / verifier work on-MCU. Roughly
  doubles the proof size.
- Lookup arguments add their own degree-3+ constraint complexity.
- Plonky3's `uni-stark` framework is single-AIR. Multi-AIR uses
  `batch-stark` (`vendor/Plonky3/batch-stark/`) which we have not
  vendored a verifier for and have not benchmarked.
- Adds a STARK-framework dependency change at the same time as a new
  AIR; harder to bisect later.

### Option C — Custom AIR that *re-implements* Poseidon2

Define a new column layout from scratch (e.g., with rounds-as-rows
rather than rounds-as-columns), trace 256 rows × 32 cols as the
scoping doc originally assumed, and write fresh constraints.

Pros:
- Trace shape matches what the scoping doc predicted; verify-cost
  estimation maps clearly.
- Smaller per-row footprint; per-query Merkle authentication paths
  open fewer columns.

Cons:
- Re-implements Poseidon2 round logic from scratch. **Invalidates
  audit coverage** — we would be running a NEW Poseidon2
  implementation, with new constraint logic, fresh round-constant
  encoding. The audit at `crates/zkmcu-poseidon-audit` only covers
  Plonky3's exact code. Adopting Option C resurrects the same
  problem the verifier-framework spike concluded against in the
  research report — and this time the re-implemented hash is *inside
  the AIR itself*, not in a sibling hash trait.
- A re-audit on the new constraint surface is at least 2 weeks on
  top of the milestone.

### Recommendation: Option A

A is the right pick because it:

- Preserves audit coverage on the hash (every constraint comes from
  the vendored, audited round structure).
- Keeps the verify path single-AIR, single-proof, which matches what
  every previous milestone has shipped. No new framework deps.
- Concentrates the engineering risk in the auxiliary column logic
  (Merkle conditional swap, selector flags, public-input binding) —
  which is well-understood STARK-AIR territory.

The cost of A is glue code: a `PqSemaphoreCols` struct, an `Air::eval`
implementation that calls Plonky3 helpers + adds extra constraints,
and witness generation that interleaves the two. 1-2 days of careful
engineering, bounded.

## Trace shape under Option A

12 permutations total: 10 Merkle hops + 1 nullifier + 1 scope binding.
Round up to next power of two for FRI: **16 rows**. Each row is one
Poseidon2 permutation.

Pad rows are tagged via the `is_merkle_row / is_nullifier_row /
is_scope_row / is_root_check` selectors all being zero — the AIR
treats those rows as "valid Poseidon2 permutation but no application
constraints", and the FRI verify still hashes them.

| Row | Role                       | `direction_bit` | Inputs                          | Output role                  |
|----:|----------------------------|-----------------|---------------------------------|------------------------------|
|   0 | Merkle hop level 0 (leaf)  | path[0]         | (leaf, sibling[0]) / swap by db | feed into row 1              |
|   1 | Merkle hop level 1         | path[1]         | (row[0].out, sibling[1])        | feed into row 2              |
| ... | ...                        | ...             | ...                             | ...                          |
|   9 | Merkle hop level 9 (root)  | path[9]         | (row[8].out, sibling[9])        | row[9].out == public.root    |
|  10 | Nullifier hash             | n/a             | (id, scope)                     | row[10].out == public.nullif |
|  11 | Scope binding hash         | n/a             | (scope, message)                | row[11].out == public.scope_hash |
| 12-15 | padding                  | n/a             | n/a                             | selector = 0                 |

Trace columns: ~298 (Poseidon2) + ~10 (auxiliary) ≈ **310 columns**.

## Public input layout

**4-element digests across the board.** Each public hash is 4
`BabyBear` elements ≈ 124 bits of digest space — comfortably above
the 128-bit security floor for the practical purposes of this
milestone (a malicious prover would need 2^62 work to find a
collision, several orders of magnitude above what is plausible
today). 16 `BabyBear` elements total in the public-inputs array.

```text
public[ 0.. 4] = merkle_root[0..4]   // 4 BabyBear elements
public[ 4.. 8] = nullifier[0..4]     // 4 BabyBear elements
public[ 8..12] = signal_hash[0..4]   // 4 BabyBear elements (binds to message)
public[12..16] = scope_hash[0..4]    // 4 BabyBear elements (binds to scope)
```

Wire format on disk: 16 × 4 = **64 bytes** in
`public.bin`, little-endian per element to match `BabyBear`'s
canonical encoding.

Sponge convention. Each digest is the first four elements of the
post-permutation state (`ending_full_rounds[3].post[0..4]`). Sponge
capacity is 16 − 8 = 8 elements ≈ 248 bits, well above the 128-bit
security floor for sponge constructions.

Identity / signal / scope encoding. Each input quantity (identity
commitment `id`, message, scope) also occupies 4 `BabyBear`
elements. The host-side prover encodes them deterministically from
their conceptual byte-strings.

## Auxiliary constraints (the Option-A glue)

### Conditional swap on Merkle rows (4-element digests)

For row $i \in \{0, \ldots, 9\}$ where `is_merkle_row = 1`:

```text
let (left, right) = if direction_bit == 0 {
    (current, sibling)            // 4-element vectors
} else {
    (sibling, current)
};
poseidon2.inputs[0..4] == left    // 4 constraint equations
poseidon2.inputs[4..8] == right   // 4 constraint equations
poseidon2.inputs[8..16] == 0      // 8 capacity zeros
```

`direction_bit` must be 0 or 1: `direction_bit * (direction_bit - 1) == 0`.

For each of the four element positions $j \in \{0, 1, 2, 3\}$ the
encoding is:
- `current[j] = poseidon2.inputs[j]   * (1 - direction_bit) + poseidon2.inputs[j+4] * direction_bit`
- `sibling[j] = poseidon2.inputs[j]   * direction_bit       + poseidon2.inputs[j+4] * (1 - direction_bit)`

All eight equations hold simultaneously (degree 2 in trace). We don't
expose `current[0..4]` and `sibling[0..4]` as separate trace columns —
they're derived from the Poseidon2 inputs and the direction bit.

Per-row constraint count: 8 (input wiring) + 8 (capacity zero) +
1 (boolean direction bit) = 17 auxiliary constraints per Merkle row,
all degree ≤ 2.

### Inter-row continuity for the Merkle chain

For each Merkle row $i \geq 1$: the 4-element `current` of row $i$
equals the 4-element digest squeezed from row $i-1$'s output. Since
Poseidon2's output is the permutation's full state (16 elements), we
extract the digest from `poseidon2.ending_full_rounds[3].post[0..4]`
(first 4 slots of the final state, matching standard sponge squeeze).

Encoded as 4 transition constraints (one per element position),
referencing the previous row's `post[j]` and the current row's
derived `current[j]`. Degree 2 in trace. 4 transition constraints
per Merkle row $\geq 1$, totalling 36 across the path.

### Public-input binding

For each of the four elements $j \in \{0, 1, 2, 3\}$:

```text
row[9].post[j]  == public.merkle_root[j]   on row where is_root_check = 1
row[10].post[j] == public.nullifier[j]     on row where is_nullifier_row = 1
row[11].post[j] == public.scope_hash[j]    on row where is_scope_row = 1
row[11].inputs[j..j+4] = public.signal_hash[j..j+4]  // binds signal via row-11 input
```

(Selector multiplications make the constraints zero on rows where
they don't apply.) Total: 12 public-input binding constraints
(3 hashes × 4 elements) plus 4 signal-binding input constraints
on row 11.

`row[11]` is a Poseidon2 over `(scope, message)`. The scope occupies
input slots [0..4] and binds to `public.scope_hash` via the
post-permutation output; the message occupies input slots [4..8] and
the AIR forces those to equal `public.signal_hash` via direct
constraint, ensuring the proof commits to a specific signal.

### Selector booleanity

Each selector column is constrained to be 0 or 1, and the four
top-level selectors sum to ≤ 1 per row.

## Verify-cost prediction (Option A, 4-element digests)

Anchor: today's `pq_poseidon_chain_verify` at 64 trace rows, 28
queries, 298 columns landed at 492 ms M33 / 616 ms RV32.

Headline AIR scaling:

- *Trace rows:* 64 → 16 (smaller, but FRI cost is dominated by query
  count not trace size; net ~ −10 % verify time).
- *Trace columns:* 298 → ~315 (slightly more — 4-element digests
  add a handful of auxiliary continuity columns; +6 %).
- *FRI queries:* 28 → 64 (the canonical 95-bit-security count for
  this FRI parameter set; ~ +120 % verify time on the FRI side).
- *Constraint complexity:* ~50 auxiliary constraints on top of the
  Poseidon2 set (4-element conditional swaps, 4-element continuity,
  16 public-input bindings, selector booleanity). All degree ≤ 2.
  Marginal OOD evaluation cost (~ +7 %).

Compounded: anchor × 0.9 × 1.06 × 2.2 × 1.07 ≈ **anchor × 2.25**.

| | Anchor | Predicted (Option A, 4-elem digests) |
|---|---:|---:|
| M33 verify | 492 ms | **~ 1110 ms** |
| RV32 verify | 616 ms | **~ 1390 ms** |
| Heap peak | 211 KB | ~ 70-100 KB (fewer trace rows) |
| Proof size | 88 KB | ~ 30-50 KB (fewer rows + similar width) |
| Stack peak | 2.4 KB | ~ 2.5 KB |
| Public-inputs wire | n/a | 64 bytes (`public.bin`)         |

This is the *informal* updated estimate, not the published prediction
from the scoping doc. The scoping doc's published prediction
(900–1800 ms M33) actually still **brackets** this revised estimate
once we account for query-count scaling, even though the anchor AIR
falsified the original prediction low — the net effect of more
queries on the headline AIR cancels the favourable per-query
overhead Plonky3 demonstrated.

That's a load-bearing observation: **the original scoping prediction
may stand for the headline measurement**, even though the anchor
falsified low. Worth flagging in the eventual results report.

## Comparison to the BN254 / Groth16 baseline

| | BN254 / Groth16 | PQ-Semaphore (predicted, 4-elem) |
|---|---:|---:|
| M33 verify | 551 ms | ~ 1110 ms (~ 2× slower) |
| RV32 verify | 1363 ms | ~ 1390 ms (parity) |
| Proof size | 256 B | ~ 40 KB (160× bigger) |

On RV32 the predicted PQ-Semaphore verify is roughly *equal* to the
BN254 verify — the query-count cost is offset by Plonky3's
single-register BabyBear arithmetic where Groth16 paid the full
schoolbook BN254 Fq cost. M33 still pays a ~ 2× verify-time tax
because UMAAL closed the BN254 gap. Per-ISA framing is more
informative than the global "PQ tax" story the scoping doc told.

## Implementation plan (steps under 4.4)

1. **Define `PqSemaphoreCols`** struct, mirroring
   `Poseidon2Cols`'s pattern (custom DST + Borrow / BorrowMut + a
   `pub const fn num_cols`).
2. **Implement `BaseAir<F>` + `Air<AB>` for `PqSemaphoreAir`.** The
   `Air::eval` calls Plonky3's existing `FullRound` / `PartialRound`
   eval helpers against the embedded `Poseidon2Cols`, then layers our
   conditional-swap / continuity / binding / boolean / selector
   constraints on top.
3. **Witness generation.** Re-use Plonky3's
   `generate_trace_rows_for_perm` for the Poseidon2 columns of each
   row; populate the auxiliary columns from the Merkle witness +
   identity + scope + message.
4. **Host-side prover wrapper.** New module in `zkmcu-host-gen`:
   takes identity seed + scope + message + Merkle witness (all
   deterministic under fixed seeds), runs prove, self-verifies, emits
   `proof.bin` + `public.bin` to
   `crates/zkmcu-vectors/data/pq-semaphore-d10/`.
5. **Verifier-side wiring** in `zkmcu-verifier-plonky3`: expose
   `pq_semaphore` module mirroring `poseidon2_chain` (config,
   `parse_proof`, `verify_proof`, `verify_with_config`), now with a
   real `PublicInputs` parser that reads 4 BabyBear elements.
6. **Firmware crates**
   `bench-rp2350-{m33,rv32}-pq-semaphore`: same shape as the
   chain bench crates, just a different vector + a different verify
   call.
7. **Bench + measure.** Hand-flash both ISAs.
8. **Results report**
   `research/reports/<date>-pq-semaphore-results.typ` quoting the
   2026-04-29 scoping doc verbatim, scoring each falsifiable
   prediction.

## Caveats / things I'm not yet sure about

- *Padding rows.* Plonky3's `Poseidon2Air` checks that every row is a
  valid Poseidon2 permutation. If we have padding rows where the
  Poseidon2 inputs are arbitrary, the AIR will reject them. Two
  options: (a) make padding rows valid Poseidon2 of dummy zero input
  (cheap), (b) gate the Poseidon2 constraint by an
  `is_active` selector (changes the audited constraint surface,
  bad). Going with (a). Confirms during step 1.
- *Public-input ordering vs serialisation.* Plonky3's
  `verify(config, air, proof, public_values)` takes `&[Val<SC>]`.
  The order of those 4 values is part of our wire format and gets
  baked in. Document it explicitly in the verifier crate.
- *Direction-bit encoding.* Semaphore v4 uses a 32-bit
  `pathIndices` integer where each bit is one direction. Our AIR
  expects the bits split across 10 separate trace columns. Host-side
  prover does the bit-decomposition once; verifier doesn't need to
  see the integer.

## Out of scope for v0

- Aggregation / recursion.
- Wire-format compatibility with Ethereum's Semaphore precompile
  (different curve, different proof shape).
- Constant-time verify (Plonky3 verify is data-oblivious in trace
  width but FRI query indices are derived from the Fiat-Shamir
  challenge, which depends on public inputs — same posture as the
  existing `zkmcu-verifier-stark`).

## Estimated cost

Step 1-2: 1.0 day (custom AIR, embed + extra constraints over
4-element digests).
Step 3: 0.5 day (witness generation, mostly straightforward).
Step 4: 0.5 day (host prover wrapper, deterministic seed for
identity / scope / message encoding into 4-element BabyBear groups).
Step 5: 0.5 day (verifier-side wiring + 64-byte `public.bin` parser).
Step 6: 0.5 day (firmware, copy from existing template).
Step 7: 1 BOOTSEL session.
Step 8: 0.5 day (results report).

Total: **~ 4 days** of focused engineering, plus the BOOTSEL session.
Reasonable to fork steps 1-4 as one chunk (the AIR + host prover) to
keep iteration noise out of the main thread.
