//! Structural tests for the committed test-vector fixtures.
//!
//! These tests verify that each `pub fn` loader returns `Ok` and that the
//! parsed structure matches the circuit's advertised shape (IC table size,
//! public-input count). End-to-end verify coverage lives in the verifier
//! crates' own test suites, this file is the gate *at the loader layer*,
//! so a fixture that goes rotten (bad bytes committed, wrong circuit
//! regenerated, `include_bytes` path drifts) fails here with a clear signal
//! before it confuses downstream tests.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::must_use_candidate,
    clippy::integer_division
)]

use zkmcu_vectors::{semaphore_depth_10, square, squares_5, UMAAL_KAT, UMAAL_KAT_RECORD_SIZE};

#[test]
fn square_loads_with_expected_shape() {
    let v = square().expect("square fixture parses");
    assert_eq!(v.name, "square");
    assert_eq!(v.public.len(), 1, "square circuit has one public input");
    assert_eq!(
        v.vk.ic.len(),
        v.public.len() + 1,
        "ic table must hold public_count + 1 entries"
    );
}

#[test]
fn squares_5_loads_with_expected_shape() {
    let v = squares_5().expect("squares-5 fixture parses");
    assert_eq!(v.name, "squares-5");
    assert_eq!(v.public.len(), 5);
    assert_eq!(v.vk.ic.len(), 6);
}

#[test]
fn semaphore_depth_10_loads_with_expected_shape() {
    let v = semaphore_depth_10().expect("semaphore depth-10 fixture parses");
    assert_eq!(v.name, "semaphore-depth-10");
    // Semaphore: [merkleTreeRoot, nullifier, hash(message), hash(scope)] = 4 inputs.
    assert_eq!(v.public.len(), 4);
    assert_eq!(v.vk.ic.len(), 5);
}

#[test]
fn umaal_kat_is_non_empty_and_record_aligned() {
    assert!(!UMAAL_KAT.is_empty(), "KAT fixture is missing");
    assert_eq!(
        UMAAL_KAT.len() % UMAAL_KAT_RECORD_SIZE,
        0,
        "KAT size must be a multiple of record size (a, b, a*b = 96 B)"
    );
    let records = UMAAL_KAT.len() / UMAAL_KAT_RECORD_SIZE;
    // Host-gen writes N=256 records; if that ever changes consciously,
    // this test is the natural update point.
    assert_eq!(records, 256, "expected 256 KAT records, got {records}");
}
