//! Host-side wall-clock parity guard for the constant-time dual verifier.
//!
//! The 2026-05-03 ct-reject bench on M33 surfaced a 9.46x timing leak on
//! `Mutation::M5_public_byte` — public-input flips desynced the Plonky3
//! challenger and tripped an early-return inside `p3-fri::verify_fri`'s
//! commit-phase Merkle check. The pre-existing
//! `ct_matches_phase_c_on_every_mutation` checked boolean parity but never
//! looked at duration, so M5 slipped through the host gate and was only
//! caught on-silicon.
//!
//! This test closes that hole. For every mutation in
//! `zkmcu_vectors::mutations::ALL` it measures `verify_constant_time`
//! wall-clock duration and asserts each is within a coarse multiplicative
//! tolerance of the honest baseline. The on-silicon ct-reject bench remains
//! the precision instrument; this is a regression net.
//!
//! Tolerance is intentionally generous (3x) so noisy CI shouldn't trip it,
//! but the M5 leak was 9.46x — well outside any reasonable host noise floor.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::integer_division,
    clippy::similar_names,
    clippy::print_stderr,
    clippy::manual_assert
)]

use std::time::{Duration, Instant};

use zkmcu_vectors::mutations::ALL;
use zkmcu_verifier_plonky3::pq_semaphore_dual_ct::verify_constant_time;

static DUAL_PROOF_P2: &[u8] =
    include_bytes!("../../zkmcu-vectors/data/pq-semaphore-d10-dual/proof_p2.bin");
static DUAL_PROOF_B3: &[u8] =
    include_bytes!("../../zkmcu-vectors/data/pq-semaphore-d10-dual/proof_b3.bin");
static DUAL_PUBLIC: &[u8] =
    include_bytes!("../../zkmcu-vectors/data/pq-semaphore-d10-dual/public.bin");

const ITERATIONS: usize = 3;
const RATIO_TOLERANCE: f64 = 3.0;

fn time_verify(proof_p2: &[u8], proof_b3: &[u8], public: &[u8]) -> Duration {
    // Median of `ITERATIONS` runs to suppress single-run scheduling noise.
    let mut durations: Vec<Duration> = (0..ITERATIONS)
        .map(|_| {
            let start = Instant::now();
            let _ = verify_constant_time(proof_p2, proof_b3, public);
            start.elapsed()
        })
        .collect();
    durations.sort();
    durations[ITERATIONS / 2]
}

#[test]
fn ct_wall_clock_matches_honest_within_tolerance() {
    let honest_duration = time_verify(DUAL_PROOF_P2, DUAL_PROOF_B3, DUAL_PUBLIC);
    let honest_secs = honest_duration.as_secs_f64();

    let mut report = vec![format!(
        "honest_baseline median over {ITERATIONS} runs: {honest_secs:.3}s"
    )];
    let mut failures = vec![];

    for mutation in ALL {
        let mut proof_p2 = DUAL_PROOF_P2.to_vec();
        let mut public = DUAL_PUBLIC.to_vec();
        mutation.apply(&mut proof_p2, &mut public);

        let mutated_duration = time_verify(&proof_p2, DUAL_PROOF_B3, &public);
        let mutated_secs = mutated_duration.as_secs_f64();
        let ratio = mutated_secs / honest_secs;

        report.push(format!(
            "{:30} {:.3}s  ratio={:.4}",
            mutation.name(),
            mutated_secs,
            ratio,
        ));

        if !(1.0 / RATIO_TOLERANCE..=RATIO_TOLERANCE).contains(&ratio) {
            failures.push(format!(
                "{}: {:.3}s vs honest {:.3}s (ratio {:.4} outside [{:.4}, {:.4}])",
                mutation.name(),
                mutated_secs,
                honest_secs,
                ratio,
                1.0 / RATIO_TOLERANCE,
                RATIO_TOLERANCE,
            ));
        }
    }

    eprintln!("{}", report.join("\n"));
    if !failures.is_empty() {
        panic!(
            "wall-clock parity failed for {} mutation(s):\n{}\n\nfull report:\n{}",
            failures.len(),
            failures.join("\n"),
            report.join("\n"),
        );
    }
}
