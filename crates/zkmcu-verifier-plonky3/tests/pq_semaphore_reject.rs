//! End-to-end check that every adversarial mutation in
//! `zkmcu_vectors::mutations::ALL` either rejects or — for `Mutation::None` —
//! accepts. Phase C measurement-only firmware burns time on flashes; this
//! test catches a stale pattern set before we get to the hardware.

#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]

use zkmcu_vectors::mutations::{Mutation, ALL};
use zkmcu_verifier_plonky3::pq_semaphore::parse_and_verify;

static COMMITTED_PROOF: &[u8] =
    include_bytes!("../../zkmcu-vectors/data/pq-semaphore-d10/proof.bin");
static COMMITTED_PUBLIC: &[u8] =
    include_bytes!("../../zkmcu-vectors/data/pq-semaphore-d10/public.bin");

#[test]
fn honest_path_accepts() {
    let mut proof = COMMITTED_PROOF.to_vec();
    let mut public = COMMITTED_PUBLIC.to_vec();
    Mutation::None.apply(&mut proof, &mut public);
    parse_and_verify(&proof, &public).expect("honest verify accepts");
}

#[test]
fn every_mutation_rejects() {
    for mutation in ALL {
        if matches!(mutation, Mutation::None) {
            continue;
        }
        let mut proof = COMMITTED_PROOF.to_vec();
        let mut public = COMMITTED_PUBLIC.to_vec();
        mutation.apply(&mut proof, &mut public);
        assert!(
            parse_and_verify(&proof, &public).is_err(),
            "{}: honest accept on mutated input",
            mutation.name(),
        );
    }
}
