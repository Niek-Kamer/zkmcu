#import "/research/lib/template.typ": *

#show: paper.with(
  title: "STARK verifier panicked on adversarial input",
  authors: ("zkmcu",),
  date: "2026-04-24",
  kind: "postmortem",
  abstract: [
    Property tests on `zkmcu-verifier-stark` found two panic paths reachable from untrusted bytes: one at verify-time (cross-field element size mismatch) and one at parse-time (`TraceInfo::new_multi_segment` assertion on `trace_length < 8`). On host the test runner catches the panic; on firmware under `panic-halt` either one is a DoS with a ~100-byte payload. The `SECURITY.md` threat model says `verify` must never crash on adversarial input, so this was a direct violation. Closed same day via wrapper pre-checks in each AIR's verify and a `sanity_check_proof_header` in `parse_proof`.
  ],
)

= Path 1: verify-time field-size mismatch

`fibonacci_babybear::verify(goldilocks_proof, babybear_public)` panics inside winterfell at `vendor/winterfell/math/src/field/traits.rs:273`:

```rust
fn from_bytes_with_padding(bytes: &[u8]) -> Self {
    assert!(bytes.len() < Self::ELEMENT_BYTES); // panics here
    ...
}
```

Goldilocks's `BaseElement::ELEMENT_BYTES = 8`. BabyBear's `ELEMENT_BYTES = 4`. When the babybear verifier deserialises a goldilocks proof and tries to interpret one of the 8-byte field chunks as a BabyBear element, the assertion fails because `8 >= 4`.

The reverse direction (goldilocks verifier on a babybear proof) does *not* panic because `4 < 8` satisfies the assertion and padding zero-extends the 4 bytes into a valid 8-byte slot. It fails at verify-time with a proper `Err`, wich is what we want everywhere.

= Path 2: parse-time trace-length assertion

`parse_proof(bytes)` panics on byte sequences that decode far enough to construct a `TraceInfo` with `trace_length < 8`. Source: `vendor/winterfell/air/src/air/trace_info.rs:91`:

```rust
assert!(trace_length >= Self::MIN_TRACE_LENGTH, ...);
```

Found via proptest on two byte-size buckets (8 KB and 30-35 KB, matching real proof sizes). Both trip it within ~50 cases. The winterfell wire format has enough "escape hatches" that random bytes can drive the deserializer into the `TraceInfo` constructor with crafted values.

This is *worse* than Path 1 because it happens at parse time, before any semantic validation or cryptographic work. An attacker just has to send bytes, no valid proof structure required.

= Scope

Any AIR pair where one field has strictly larger `ELEMENT_BYTES` than the other is exposed in the direction large → small. On this repo today that's goldilocks proofs hitting the babybear verifier. Both verifiers are compiled into the same bench firmware (`bench-rp2350-m33-stark --features babybear`), so the hole is reachable any time the firmware parses a proof whose field doesn't match the compiled AIR.

= Fix

- *Path 2 (parse-time)* closed by `sanity_check_proof_header` in `zkmcu-verifier-stark::parse_proof`. The check screens the first 4 bytes of the proof (main width, aux width, aux rands, `trace_length_log2`) against the two invariants `TraceInfo::new_multi_segment` asserts but `TraceInfo::read_from` doesn't: `aux == 0 → rands == 0`, and `trace_length_log2 ∈ [3, 62]`. The upper bound guards the `2_usize.pow(log2)` overflow on 64-bit that wraps to 0 and then trips the `>= 8` assert.
- *Path 1 (verify-time)* closed in each AIR's verify. `fibonacci::verify` and `fibonacci_babybear::verify` now compare `proof.context.field_modulus_bytes()` against `BaseField::get_modulus_le_bytes()` before handing the proof to `winterfell::verify`. A cross-field proof returns `Error::ProofDeserialization` before any call into `from_bytes_with_padding`.

= Verification

- `parse_proof_never_panics` proptest (256 cases × 0..8192 bytes): passes.
- `parse_proof_never_panics_on_proof_sized_garbage` proptest (256 cases × 30_000..35_000 bytes): passes.
- `babybear_verifier_rejects_goldilocks_proof`: passes with a plain match, no `catch_unwind` needed.
- STARK properties suite is 4/4, all `#[ignore]` attributes removed.

= Upstream

Both winterfell asserts (`new_multi_segment` trace_length and aux/rands invariants, `from_bytes_with_padding` byte length) should return `Result` on deserialisation paths rather than panicking. Filing against `facebook/winterfell` would protect every downstream user. Deferred, not blocking.

See also: 2026-04-24-stark-unbounded-vec-alloc (third and fourth panic paths, found by fuzz after these two were closed).
