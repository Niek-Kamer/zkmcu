//! Threshold-check STARK prover on Cortex-M33.
//!
//! Proves `value < threshold` using the bit-decomposition AIR from
//! `zkmcu-verifier-stark::threshold_check`. Both value and threshold are
//! public; this is verifiable computation, not zero-knowledge.
//!
//! Circuit: 2 columns × 64 rows. Bit-decomposes `diff = threshold - value - 1`
//! over 32 rows; boundary assertion `remaining[32] = 0` certifies no
//! field-element underflow, hence `value < threshold`.
//!
//! Companion to `bench-rp2350-m33-stark-prover-bb` (Fibonacci BabyBear).
//! Uses the same BabyBear+Quartic field configuration; the threshold AIR is
//! 64 rows vs 256, so prove and heap should be dramatically smaller.

#![no_std]
#![no_main]
#![allow(clippy::integer_division)]
#![allow(clippy::panic)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::indexing_slicing)]
#![allow(clippy::doc_markdown)]

extern crate alloc;

use alloc::vec;

use core::fmt::Write as _;
use core::mem::MaybeUninit;

use bench_core::{
    init_cycle_counter, init_rp2350, measure_cycles, measure_stack_peak, Bench, BenchConfig,
    TrackingTlsf, UsbBus, SYS_HZ,
};
use heapless::String;
use panic_halt as _;
use rp235x_hal as hal;

use zkmcu_babybear::BaseElement;

use winterfell::crypto::hashers::Blake3_256;
use winterfell::crypto::{DefaultRandomCoin, MerkleTree};
use winterfell::math::FieldElement;
use winterfell::matrix::ColMatrix;
use winterfell::{
    AcceptableOptions, AuxRandElements, BatchingMethod, CompositionPoly, CompositionPolyTrace,
    ConstraintCompositionCoefficients, DefaultConstraintCommitment, DefaultConstraintEvaluator,
    DefaultTraceLde, FieldExtension, PartitionOptions, ProofOptions, Prover, StarkDomain,
    TraceInfo, TracePolyTable, TraceTable,
};

use zkmcu_verifier_stark::threshold_check::{
    build_trace, PublicInputs, ThresholdAir, TRACE_LEN,
};

/// Sensor reading to prove below threshold.
const VALUE: u32 = 37;
/// Safety threshold; circuit proves VALUE < THRESHOLD.
const THRESHOLD: u32 = 100;

#[global_allocator]
static HEAP: TrackingTlsf = TrackingTlsf::empty();

const HEAP_SIZE: usize = 384 * 1024;
static mut HEAP_MEM: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];

#[link_section = ".start_block"]
#[used]
pub static IMAGE_DEF: hal::block::ImageDef = hal::block::ImageDef::secure_exe();

#[link_section = ".bi_entries"]
#[used]
pub static PICOTOOL_ENTRIES: [hal::binary_info::EntryAddr; 4] = [
    hal::binary_info::rp_cargo_bin_name!(),
    hal::binary_info::rp_cargo_version!(),
    hal::binary_info::rp_program_description!(
        c"zkmcu: STARK threshold-check PROVER BabyBear+Quartic (Cortex-M33)"
    ),
    hal::binary_info::rp_program_build_attribute!(),
];

// Trace length is 64 (TRACE_LEN from threshold_check).
// LDE domain = 64 × 4 = 256 elements.
// FRI folds: 256 → 64 → 16 → 4 (stop, 4/4=1 ≤ 8). 3 rounds.
// 64 queries at blowup=4: floor(log2(4)) × 64 = 2 × 64 = 128-bit conjectured security.
const fn proof_options() -> ProofOptions {
    ProofOptions::new(
        64,                      // num_queries — 128-bit conjectured security at blowup=4
        4,                       // blowup_factor
        0,                       // grinding_factor
        FieldExtension::Quartic,
        4,                       // fri_folding_factor
        7,                       // fri_max_remainder_poly_degree
        BatchingMethod::Linear,
        BatchingMethod::Linear,
    )
}

// ── Prover ────────────────────────────────────────────────────────────────

struct ThresholdProver {
    options: ProofOptions,
    value: u32,
    threshold: u32,
}

impl Prover for ThresholdProver {
    type BaseField = BaseElement;
    type Air = ThresholdAir;
    type Trace = TraceTable<BaseElement>;
    type HashFn = Blake3_256<BaseElement>;
    type VC = MerkleTree<Self::HashFn>;
    type RandomCoin = DefaultRandomCoin<Self::HashFn>;
    type TraceLde<E: FieldElement<BaseField = Self::BaseField>> =
        DefaultTraceLde<E, Self::HashFn, Self::VC>;
    type ConstraintCommitment<E: FieldElement<BaseField = Self::BaseField>> =
        DefaultConstraintCommitment<E, Self::HashFn, Self::VC>;
    type ConstraintEvaluator<'a, E: FieldElement<BaseField = Self::BaseField>> =
        DefaultConstraintEvaluator<'a, ThresholdAir, E>;

    fn get_pub_inputs(&self, _trace: &Self::Trace) -> PublicInputs {
        PublicInputs { value: self.value, threshold: self.threshold }
    }

    fn options(&self) -> &ProofOptions {
        &self.options
    }

    fn new_trace_lde<E: FieldElement<BaseField = Self::BaseField>>(
        &self,
        trace_info: &TraceInfo,
        main_trace: &ColMatrix<Self::BaseField>,
        domain: &StarkDomain<Self::BaseField>,
        partition_option: PartitionOptions,
    ) -> (Self::TraceLde<E>, TracePolyTable<E>) {
        DefaultTraceLde::new(trace_info, main_trace, domain, partition_option)
    }

    fn build_constraint_commitment<E: FieldElement<BaseField = Self::BaseField>>(
        &self,
        composition_poly_trace: CompositionPolyTrace<E>,
        num_constraint_composition_columns: usize,
        domain: &StarkDomain<Self::BaseField>,
        partition_options: PartitionOptions,
    ) -> (Self::ConstraintCommitment<E>, CompositionPoly<E>) {
        DefaultConstraintCommitment::new(
            composition_poly_trace,
            num_constraint_composition_columns,
            domain,
            partition_options,
        )
    }

    fn new_evaluator<'a, E: FieldElement<BaseField = Self::BaseField>>(
        &self,
        air: &'a ThresholdAir,
        aux_rand_elements: Option<AuxRandElements<E>>,
        composition_coefficients: ConstraintCompositionCoefficients<E>,
    ) -> Self::ConstraintEvaluator<'a, E> {
        DefaultConstraintEvaluator::new(air, aux_rand_elements, composition_coefficients)
    }
}

// ── Entry point ──────────────────────────────────────────────────────────

#[hal::entry]
fn main() -> ! {
    // SAFETY: unique static address, called exactly once before any alloc.
    unsafe { HEAP.init(core::ptr::addr_of_mut!(HEAP_MEM) as usize, HEAP_SIZE) }

    init_cycle_counter();
    let pac = hal::pac::Peripherals::take().expect("rp235x PAC once");
    let (timer, usb_bus) = init_rp2350(pac);

    let mut bench = Bench::new(
        &usb_bus,
        timer,
        BenchConfig {
            manufacturer: "zkmcu",
            product: "bench-rp2350-m33-stark-prover-threshold",
            serial: "0001",
        },
    );

    bench.enumerate_for(2_000_000);

    let mut boot_line: String<192> = String::new();
    let _ = writeln!(
        &mut boot_line,
        "zkmcu boot: heap=384K sys=150MHz core=cortex-m33 alloc=tlsf stark-prover-threshold N={TRACE_LEN} value={VALUE} threshold={THRESHOLD}\r",
    );
    bench.write_line(boot_line.as_bytes());

    let sys_hz: u64 = u64::from(SYS_HZ);

    boot_measure(&mut bench, sys_hz);

    let mut iter: u32 = 0;
    loop {
        iter = iter.wrapping_add(1);

        bench.print_marker(iter, b"stark_prove start\r\n");
        bench.pace(200_000);

        let (prove_result, prove_cycles) = measure_cycles(|| {
            let trace = build_trace(VALUE, THRESHOLD);
            let prover = ThresholdProver { options: proof_options(), value: VALUE, threshold: THRESHOLD };
            prover.prove(trace)
        });

        let prove_us = prove_cycles.saturating_mul(1_000_000) / sys_hz;
        let (prove_ok, proof) = prove_result.map_or(("ok=false", None), |p| ("ok=true", Some(p)));
        let proof_bytes = proof.as_ref().map_or(0, |p| p.to_bytes().len());
        let security_bits = proof.as_ref().map_or(0u32, |p| {
            p.conjectured_security::<Blake3_256<BaseElement>>().bits()
        });

        let mut out: String<224> = String::new();
        let _ = writeln!(
            &mut out,
            "[{iter}] stark_prove: cycles={prove_cycles} us={prove_us} ms={} proof_bytes={proof_bytes} security_bits={security_bits} {prove_ok}",
            prove_us / 1000,
        );
        bench.write_line(out.as_bytes());

        if let Some(p) = proof {
            let public = PublicInputs { value: VALUE, threshold: THRESHOLD };
            let opts = AcceptableOptions::OptionSet(vec![proof_options()]);
            let (verify_result, verify_cycles) = measure_cycles(|| {
                winterfell::verify::<ThresholdAir, Blake3_256<BaseElement>,
                    DefaultRandomCoin<Blake3_256<BaseElement>>,
                    MerkleTree<Blake3_256<BaseElement>>>(p, public, &opts)
            });

            let verify_us = verify_cycles.saturating_mul(1_000_000) / sys_hz;
            let verdict = match verify_result {
                Ok(()) => "ok=true",
                Err(_) => "ok=false",
            };

            let mut vout: String<160> = String::new();
            let _ = writeln!(
                &mut vout,
                "[{iter}] stark_verify: cycles={verify_cycles} us={verify_us} ms={} {verdict}",
                verify_us / 1000,
            );
            bench.write_line(vout.as_bytes());

            let total_cycles = prove_cycles.saturating_add(verify_cycles);
            let total_us = total_cycles.saturating_mul(1_000_000) / sys_hz;
            let heap_after = HEAP.current();
            let mut tout: String<192> = String::new();
            let _ = writeln!(
                &mut tout,
                "[{iter}] prove_verify_total: prove_us={prove_us} verify_us={verify_us} total_us={total_us} total_ms={} heap_after={heap_after} {verdict}",
                total_us / 1000,
            );
            bench.write_line(tout.as_bytes());
        }

        bench.pace(30_000_000);
    }
}

fn boot_measure<B: UsbBus>(bench: &mut Bench<'_, B>, sys_hz: u64) {
    bench.write_line(b"[boot] prove start\r\n");
    bench.pace(200_000);

    HEAP.reset_peak();
    let heap_before = HEAP.current();

    let (prove_result, stack_peak, prove_cycles) = measure_stack_peak(|| {
        let trace = build_trace(VALUE, THRESHOLD);
        let prover = ThresholdProver { options: proof_options(), value: VALUE, threshold: THRESHOLD };
        prover.prove(trace)
    });

    let heap_peak = HEAP.peak();
    let stack_bytes = stack_peak.unwrap_or(0);
    let prove_us = prove_cycles.saturating_mul(1_000_000) / sys_hz;
    let (prove_ok, proof) = prove_result.map_or(("ok=false", None), |p| ("ok=true", Some(p)));
    let proof_bytes = proof.as_ref().map_or(0, |p| p.to_bytes().len());
    let security_bits = proof.as_ref().map_or(0u32, |p| {
        p.conjectured_security::<Blake3_256<BaseElement>>().bits()
    });

    let mut out: String<320> = String::new();
    let _ = writeln!(
        &mut out,
        "[boot] N={TRACE_LEN} stack={stack_bytes} heap_base={heap_before} heap_peak={heap_peak} cycles={prove_cycles} us={prove_us} ms={} proof_bytes={proof_bytes} security_bits={security_bits} {prove_ok}\r",
        prove_us / 1000,
    );
    bench.write_line(out.as_bytes());

    if let Some(p) = proof {
        let public = PublicInputs { value: VALUE, threshold: THRESHOLD };
        let opts = AcceptableOptions::OptionSet(vec![proof_options()]);
        let (verify_result, verify_cycles) = measure_cycles(|| {
            winterfell::verify::<ThresholdAir, Blake3_256<BaseElement>,
                DefaultRandomCoin<Blake3_256<BaseElement>>,
                MerkleTree<Blake3_256<BaseElement>>>(p, public, &opts)
        });
        let verify_us = verify_cycles.saturating_mul(1_000_000) / sys_hz;

        let verdict = match verify_result {
            Ok(()) => "verify=ok",
            Err(_) => "verify=FAIL",
        };

        let mut vout: String<96> = String::new();
        let _ = writeln!(&mut vout, "[boot] self-{verdict} verify_us={verify_us}\r");
        bench.write_line(vout.as_bytes());

        let total_cycles = prove_cycles.saturating_add(verify_cycles);
        let total_us = total_cycles.saturating_mul(1_000_000) / sys_hz;
        let heap_after = HEAP.current();
        let mut tout: String<128> = String::new();
        let _ = writeln!(
            &mut tout,
            "[boot] prove_verify_total: total_us={total_us} total_ms={} heap_after={heap_after}\r",
            total_us / 1000,
        );
        bench.write_line(tout.as_bytes());
    }
}
