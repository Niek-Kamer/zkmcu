//! Differential test: our reference Poseidon2 permutation vs Plonky3's.
//!
//! Both implementations are driven with the same published `BabyBear`
//! Poseidon2 round constants
//! (`BABYBEAR_POSEIDON2_RC_16_EXTERNAL_INITIAL`/`_FINAL` and
//! `BABYBEAR_POSEIDON2_RC_16_INTERNAL` from `p3-baby-bear`). For each
//! input vector we apply both perms and assert byte-identical outputs.
//!
//! This is the load-bearing implementation-correctness test for the
//! audit. Slices 1-3 verified the design (round counts, MDS, V vector).
//! This slice verifies that our reference impl matches Plonky3's
//! production implementation on the exact same constants.

#![allow(
    clippy::indexing_slicing,
    clippy::cast_possible_truncation,
    clippy::needless_range_loop,
    clippy::doc_markdown
)]

use p3_baby_bear::{
    BabyBear, Poseidon2BabyBear, BABYBEAR_POSEIDON2_RC_16_EXTERNAL_FINAL,
    BABYBEAR_POSEIDON2_RC_16_EXTERNAL_INITIAL, BABYBEAR_POSEIDON2_RC_16_INTERNAL,
};
use p3_field::{PrimeCharacteristicRing, PrimeField32};
use p3_poseidon2::ExternalLayerConstants;
use p3_symmetric::Permutation;

use zkmcu_poseidon_audit::perm::poseidon2_permute_t16_babybear;

const T: usize = 16;
const HALF_RF: usize = 4;
const RP: usize = 13;

fn babybear_array_to_u64(arr: &[BabyBear; T]) -> [u64; T] {
    let mut out = [0_u64; T];
    for i in 0..T {
        out[i] = u64::from(arr[i].as_canonical_u32());
    }
    out
}

fn u64_array_to_babybear(arr: &[u64; T]) -> [BabyBear; T] {
    let mut out = [BabyBear::ZERO; T];
    for i in 0..T {
        out[i] = BabyBear::new(arr[i] as u32);
    }
    out
}

/// Convert the published external initial / terminal arrays from
/// `[[BabyBear; 16]; 4]` to `[[u64; 16]; 4]` for our perm to consume.
fn convert_external_constants() -> ([[u64; T]; HALF_RF], [[u64; T]; HALF_RF]) {
    let initial = &BABYBEAR_POSEIDON2_RC_16_EXTERNAL_INITIAL;
    let terminal = &BABYBEAR_POSEIDON2_RC_16_EXTERNAL_FINAL;
    let mut conv_initial = [[0_u64; T]; HALF_RF];
    let mut conv_terminal = [[0_u64; T]; HALF_RF];
    for r in 0..HALF_RF {
        conv_initial[r] = babybear_array_to_u64(&initial[r]);
        conv_terminal[r] = babybear_array_to_u64(&terminal[r]);
    }
    (conv_initial, conv_terminal)
}

fn convert_internal_constants() -> [u64; RP] {
    let mut out = [0_u64; RP];
    for i in 0..RP {
        out[i] = u64::from(BABYBEAR_POSEIDON2_RC_16_INTERNAL[i].as_canonical_u32());
    }
    out
}

fn build_plonky3_perm() -> Poseidon2BabyBear<T> {
    let external = ExternalLayerConstants::<BabyBear, T>::new(
        BABYBEAR_POSEIDON2_RC_16_EXTERNAL_INITIAL.to_vec(),
        BABYBEAR_POSEIDON2_RC_16_EXTERNAL_FINAL.to_vec(),
    );
    let internal = BABYBEAR_POSEIDON2_RC_16_INTERNAL.to_vec();
    Poseidon2BabyBear::<T>::new(external, internal)
}

/// Run both perms on the given `u64` input vector and assert the
/// outputs are byte-identical.
fn assert_perms_agree(input: [u64; T]) {
    let plonky_perm = build_plonky3_perm();
    let mut p3_state = u64_array_to_babybear(&input);
    plonky_perm.permute_mut(&mut p3_state);
    let p3_output = babybear_array_to_u64(&p3_state);

    let (initial, terminal) = convert_external_constants();
    let internal = convert_internal_constants();

    let mut our_state = input;
    poseidon2_permute_t16_babybear(&mut our_state, &initial, &terminal, &internal);

    assert_eq!(
        our_state, p3_output,
        "our perm must match Plonky3's, input was {input:?}"
    );
}

#[test]
fn diff_against_plonky3_zero_input() {
    assert_perms_agree([0; T]);
}

#[test]
fn diff_against_plonky3_all_ones() {
    assert_perms_agree([1; T]);
}

#[test]
fn diff_against_plonky3_sequential() {
    let mut input = [0_u64; T];
    for i in 0..T {
        input[i] = i as u64;
    }
    assert_perms_agree(input);
}

#[test]
fn diff_against_plonky3_plonky_test_vector_input() {
    // The same input vector Plonky3 uses in its own
    // `test_poseidon2_width_16_random` (their test runs with
    // RNG-generated constants, ours runs with published constants,
    // so the expected output differs, but both perms must agree
    // when fed the same constants).
    let input: [u64; T] = [
        894_848_333,
        1_437_655_012,
        1_200_606_629,
        1_690_012_884,
        71_131_202,
        1_749_206_695,
        1_717_947_831,
        120_589_055,
        19_776_022,
        42_382_981,
        1_831_865_506,
        724_844_064,
        171_220_207,
        1_299_207_443,
        227_047_920,
        1_783_754_913,
    ];
    assert_perms_agree(input);
}

#[test]
fn diff_against_plonky3_max_values() {
    let max = (1_u64 << 31) - 1;
    assert_perms_agree([max; T]);
}

/// Property: changing one input slot changes (at least) the output. A
/// permutation must be injective, so different inputs cannot collide.
#[test]
fn perm_is_injective_on_single_bit_changes() {
    let base = [42_u64; T];
    let (initial, terminal) = convert_external_constants();
    let internal = convert_internal_constants();

    let mut base_out = base;
    poseidon2_permute_t16_babybear(&mut base_out, &initial, &terminal, &internal);

    for i in 0..T {
        let mut variant = base;
        variant[i] = 43;
        let mut variant_out = variant;
        poseidon2_permute_t16_babybear(&mut variant_out, &initial, &terminal, &internal);
        assert_ne!(
            base_out, variant_out,
            "single-bit change at index {i} produced identical output"
        );
    }
}
