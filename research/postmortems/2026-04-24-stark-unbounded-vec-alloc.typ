#import "/research/lib/template.typ": *

#show: paper.with(
  title: "Unbounded Vec::with_capacity in winterfell proof deserializer",
  authors: ("zkmcu",),
  date: "2026-04-24",
  kind: "postmortem",
  abstract: [
    First fuzz run on the `stark_parse_proof` target found a third panic path in the STARK verifier, deeper than proptest could reach. An attacker-controlled length prefix inside `Queries::read_from` triggers `Vec::with_capacity(2^62)` from a ~100-byte payload. On firmware the allocator fails to satisfy that request against a 96 KB heap, `alloc_error_handler` panics, device halts. Closed in our `vendor/winterfell/` fork via a `read_many` bound plus a `ProofOptions::read_from` pre-validation pass. Post-fix fuzz run: 91 M executions, zero crashes.
  ],
)

= What the fuzzer found

Stack trace (ASan build, but the same path hits on firmware):

```
alloc::raw_vec::with_capacity_in<u8, Global>
<winter_utils::serde::byte_reader::SliceReader as ByteReader>::read_many<u8>
<Vec<u8> as Deserializable>::read_from<SliceReader>
<winter_air::proof::queries::Queries as Deserializable>::read_from<SliceReader>
<winter_air::proof::Proof as Deserializable>::read_from
zkmcu_verifier_stark::parse_proof
```

`Queries::read_from` reads a length prefix and calls `Vec::with_capacity(n)` via the blanket `Deserializable for Vec<T>` impl. `n` is attacker-controlled. The crash artifacts sit at offset ~60 bytes into the proof, past several length-prefix validations that would reject garbage at the boundary. Coverage-guided mutation from a valid seed finds it; unstructured random bytes don't.

Same *class* of bug as the BN254/BLS12 `MAX_NUM_IC` cap (unbounded allocation from adversary-controlled count), but it lives deep in winterfell's deserialiser, not at our API boundary.

= Why the header pre-check didn't catch it

`sanity_check_proof_header` (shipped with the prior parse-time panic fix) validates bytes 0-3: main width, aux width, aux rands, `trace_length_log2`. That closes two panic paths but never looks past the first 4 bytes.

The Vec-capacity bug is at offset > ~60 bytes, inside a nested `Queries::read_from` call. Third distinct panic path. Same DoS class, different wire-format location.

= Interim mitigation, same day

Two pieces landed before the real fix:

+ `MAX_PROOF_SIZE = 128 KB` cap at the top of `parse_proof`. Real Fibonacci-1024 proofs are ~30 KB; 128 KB leaves generous headroom. Does *not* close the bug (the crash artifacts are well under 128 KB) but bounds the volume of parser work an attacker can force before hitting the allocator failure. Strictly better than unbounded.
+ Regression tests in `tests/adversarial.rs` that load the committed crash artifacts. Marked `#[ignore]` initially because `handle_alloc_error` aborts the whole test process; `catch_unwind` can only catch panics, not aborts. The `#[ignore]` came off once the real fix landed.

= Real fix, same day

Forked `vendor/winterfell/` (we already maintain it for the `QuartExtension` backport) with three related changes:

+ *`utils/core/src/serde/byte_reader.rs`*: new `ByteReader::remaining_bytes() -> Option<usize>` trait method. Default `None` so external impls keep compiling; `SliceReader` overrides to `Some(source.len() - pos)`. The default `read_many<D>` now caps `Vec::with_capacity(num_elements.min(remaining))` when the reader knows its remaining size. Streaming readers (`None`) fall back to pre-fix behaviour, wich is fine because they aren't the fuzz target.
+ *`air/src/options.rs`*: `ProofOptions::read_from` pre-validates every parameter (`num_queries`, `blowup_factor`, `grinding_factor`, FRI folding factor and remainder degree, `num_partitions`, `hash_rate`) against the bounds `ProofOptions::new` and `with_partitions` assert on. Returns `DeserializationError::InvalidValue` on any out-of-range byte. This caught a *fourth* panic path on the post-fix fuzz run: `num_queries = 0` bypassed deserialisation and crashed inside `ProofOptions::new`.

Both changes are small, mechanical, and worth offering upstream.

= Verification

- `cargo fuzz run stark_parse_proof` for 5 minutes: *91 498 083 executions (~300 k exec/s), zero crashes, zero artifacts.* Corpus grew from 16 seeds to ~40 entries with coverage-guided mutation.
- The two `fuzz_regression_unbounded_vec_alloc_*` regression tests pass cleanly. `#[ignore]` removed from both.
- STARK adversarial suite: 17 → 19 passing, 2 → 0 ignored.
- `just check-full` green across all six firmware builds.
- The `MAX_PROOF_SIZE = 128 KB` cap stays in place as defense-in-depth. The `read_many` fix makes it redundant for allocation DoS, but the cap also bounds parser *work* independently of allocation, so it's useful against a pure compute-exhaustion attacker too.

= Panic-on-adversarial-input, scoreboard

+ `TraceInfo::new_multi_segment` assertions. Closed via `sanity_check_proof_header`.
+ `from_bytes_with_padding` cross-field assertion. Closed via the field-modulus check in each AIR's verify.
+ `Vec::with_capacity` unbounded allocation in `read_many`. Closed in the `vendor/winterfell/` fork (this postmortem).
+ `ProofOptions::new` parameter-range assertions. Closed in the fork's `ProofOptions::read_from` pre-validation.

The crash artifacts under `crates/zkmcu-verifier-stark/tests/data/fuzz-regressions/` stay committed as a regression gate.

= Upstream plan

Offer to `facebook/winterfell` as a series of PRs:

- PR 1: `ByteReader::remaining_bytes` + `read_many` bound.
- PR 2: `ProofOptions::read_from` pre-validation (plus a similar pass over any other `new`-asserts-that-deserialisation-bypasses I find).

Our fork stays the source of truth in the meantime; rebase onto upstream main if/when patches land.

= Session-level takeaway

Proptest found two panic paths on random byte strings. Fuzz found two more that require coverage-guided mutation from a valid seed. Ofcourse random input can't reach a bug that sits 60 bytes past a length-prefix validation. Both tools pay rent: proptest for shallow parser panics, fuzz for deep structure-aware ones. We run both.
