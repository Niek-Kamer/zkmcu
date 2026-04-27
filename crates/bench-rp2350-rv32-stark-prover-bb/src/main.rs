//! Phase-5: BabyBear + Quartic-extension STARK Fibonacci prover on Hazard3 RV32.
//!
//! Companion to `bench-rp2350-rv32-stark-prover` (Phase 4, Goldilocks + None).
//! Mirrors `bench-rp2350-m33-stark-prover-bb` so results are directly comparable
//! across ISAs. BabyBear multiplication is MUL+MULHU on RV32 — symmetric to
//! UMULL on M33 — so the 1.55× cross-ISA gap seen in Phase 4 should collapse.

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
    DefaultTraceLde, FieldExtension, PartitionOptions, ProofOptions, Prover, StarkDomain, Trace,
    TraceInfo, TracePolyTable, TraceTable,
};

use zkmcu_verifier_stark::fibonacci_babybear::{FibAir, PublicInputs};

const TRACE_LEN: usize = 256;

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
        c"zkmcu: STARK Fibonacci PROVER BabyBear+Quartic (Hazard3 RV32)"
    ),
    hal::binary_info::rp_program_build_attribute!(),
];

// Stopping rule: fold until (domain / folding_factor) <= (max_remainder + 1).
// N=256, blowup=4: LDE=1024 -> 256 -> 64 -> 16 (stop, 16/4=4 <= 8). ✓
// 11 queries → ~21-bit conjectured security: floor(log2(blowup=4)) × 11 = 2×11 = 22 bits.
// 128-bit conjectured security at blowup=4 requires 64 queries; blowup=8 requires 43.
const fn proof_options() -> ProofOptions {
    ProofOptions::new(
        11,                      // num_queries — ~128-bit conjectured security target
        4,                       // blowup_factor
        0,                       // grinding_factor
        FieldExtension::Quartic, // quartic extension over 31-bit BabyBear
        4,                       // fri_folding_factor
        7,                       // fri_max_remainder_poly_degree
        BatchingMethod::Linear,
        BatchingMethod::Linear,
    )
}

// ── Prover ────────────────────────────────────────────────────────────────

struct FibProver {
    options: ProofOptions,
}

impl Prover for FibProver {
    type BaseField = BaseElement;
    type Air = FibAir;
    type Trace = TraceTable<BaseElement>;
    type HashFn = Blake3_256<BaseElement>;
    type VC = MerkleTree<Self::HashFn>;
    type RandomCoin = DefaultRandomCoin<Self::HashFn>;
    type TraceLde<E: FieldElement<BaseField = Self::BaseField>> =
        DefaultTraceLde<E, Self::HashFn, Self::VC>;
    type ConstraintCommitment<E: FieldElement<BaseField = Self::BaseField>> =
        DefaultConstraintCommitment<E, Self::HashFn, Self::VC>;
    type ConstraintEvaluator<'a, E: FieldElement<BaseField = Self::BaseField>> =
        DefaultConstraintEvaluator<'a, FibAir, E>;

    fn get_pub_inputs(&self, trace: &Self::Trace) -> PublicInputs {
        let last_step = trace.length() - 1;
        PublicInputs {
            result: trace.get(1, last_step),
        }
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
        air: &'a FibAir,
        aux_rand_elements: Option<AuxRandElements<E>>,
        composition_coefficients: ConstraintCompositionCoefficients<E>,
    ) -> Self::ConstraintEvaluator<'a, E> {
        DefaultConstraintEvaluator::new(air, aux_rand_elements, composition_coefficients)
    }
}

// ── Trace ────────────────────────────────────────────────────────────────

fn build_trace() -> TraceTable<BaseElement> {
    let mut trace = TraceTable::new(2, TRACE_LEN);
    trace.fill(
        |state| {
            state[0] = BaseElement::ONE;
            state[1] = BaseElement::ONE;
        },
        |_, state| {
            let new_s0 = state[0] + state[1];
            let new_s1 = state[1] + new_s0;
            state[0] = new_s0;
            state[1] = new_s1;
        },
    );
    trace
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
            product: "bench-rp2350-rv32-stark-prover-bb",
            serial: "0001",
        },
    );

    bench.enumerate_for(2_000_000);

    let mut boot_line: String<160> = String::new();
    let _ = writeln!(
        &mut boot_line,
        "zkmcu boot: heap=384K sys=150MHz core=hazard3-rv32 alloc=tlsf stark-prover-bb N={TRACE_LEN}\r",
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
            let trace = build_trace();
            let prover = FibProver { options: proof_options() };
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
            let public = recompute_public();
            let opts = AcceptableOptions::OptionSet(vec![proof_options()]);
            let (verify_result, verify_cycles) = measure_cycles(|| {
                winterfell::verify::<FibAir, Blake3_256<BaseElement>,
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

fn recompute_public() -> PublicInputs {
    let last = TRACE_LEN - 1;
    let mut s0 = BaseElement::ONE;
    let mut s1 = BaseElement::ONE;
    for _ in 0..last {
        let new_s0 = s0 + s1;
        let new_s1 = s1 + new_s0;
        s0 = new_s0;
        s1 = new_s1;
    }
    PublicInputs { result: s1 }
}

fn boot_measure<B: UsbBus>(bench: &mut Bench<'_, B>, sys_hz: u64) {
    bench.write_line(b"[boot] prove start\r\n");
    bench.pace(200_000);

    HEAP.reset_peak();
    let heap_before = HEAP.current();

    let (prove_result, stack_peak, prove_cycles) = measure_stack_peak(|| {
        let trace = build_trace();
        let prover = FibProver { options: proof_options() };
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
        let public = recompute_public();
        let opts = AcceptableOptions::OptionSet(vec![proof_options()]);
        let (verify_result, verify_cycles) = measure_cycles(|| {
            winterfell::verify::<FibAir, Blake3_256<BaseElement>,
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
