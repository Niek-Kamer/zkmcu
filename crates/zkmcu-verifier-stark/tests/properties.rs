//! Property-based tests for the STARK parser.
//!
//! Complements the adversarial suite: the adversarial file hits specific
//! mutation patterns (truncation, bit-flip, cross-AIR) against a known-good
//! fixture; proptest generates wholly random byte strings and asserts the
//! parsers never panic. Different failure class (panic vs. silent accept),
//! different coverage. Mirrors `zkmcu-verifier/tests/properties.rs`.
//!
//! These are parse-only properties, running a full `verify` inside a
//! proptest loop would make the test pack slow (~20 ms per verify × 256
//! cases per property = ~5 s per property). If a future audit wants
//! property-based verify fuzzing, that's a cargo-fuzz job, not proptest.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::must_use_candidate
)]

use proptest::prelude::*;

use zkmcu_verifier_stark::{fibonacci, fibonacci_babybear, parse_proof};

// `parse_proof` gets a lightweight pre-validation pass in the wrapper
// (`sanity_check_proof_header` in lib.rs) that screens the two `TraceInfo`
// byte layouts known to hit assertions in `new_multi_segment`. The
// properties below ran `#[ignore]`d while that wrapper was missing; they
// are active again now. Background: `research/postmortems/2026-04-24-stark-\
// cross-field-panic.typ`.

proptest! {
    /// `parse_proof` must never panic on any byte sequence up to 8 KB.
    /// Ok / Err are both valid outcomes; only panic is a bug.
    #[test]
    fn parse_proof_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..8192)) {
        drop(parse_proof(&bytes));
    }

    /// `fibonacci::parse_public` must never panic on any byte sequence up
    /// to 256 bytes. Wire format is fixed 8 bytes; anything else is
    /// rejected cleanly.
    #[test]
    fn parse_public_fib_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..256)) {
        drop(fibonacci::parse_public(&bytes));
    }

    /// Same property for the BabyBear variant, 4-byte public-input wire.
    #[test]
    fn parse_public_babybear_never_panics(
        bytes in proptest::collection::vec(any::<u8>(), 0..256)
    ) {
        drop(fibonacci_babybear::parse_public(&bytes));
    }

    /// `parse_proof` on arbitrary bytes that exactly match the length of a
    /// real proof fixture, a more pointed fuzz target than fully random
    /// lengths, because the deserializer gets past length-prefix checks and
    /// starts reading structure. Still: no panic.
    #[test]
    fn parse_proof_never_panics_on_proof_sized_garbage(
        bytes in proptest::collection::vec(any::<u8>(), 30_000..35_000)
    ) {
        drop(parse_proof(&bytes));
    }
}
