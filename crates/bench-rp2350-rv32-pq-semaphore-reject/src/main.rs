#![no_std]
#![no_main]
#![allow(clippy::integer_division)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::panic)]

extern crate alloc;

use alloc::vec::Vec;
use core::fmt::Write as _;
use core::mem::MaybeUninit;

use bench_core::{
    init_cycle_counter, init_rp2350, measure_cycles, Bench, BenchConfig, TrackingTlsf, UsbBus,
    SYS_HZ,
};
use heapless::String;
use panic_halt as _;
use rp235x_hal as hal;
use zkmcu_vectors::mutations::{Mutation, ALL};
use zkmcu_verifier_plonky3::pq_semaphore::{
    build_air, make_config, parse_proof, parse_public_inputs, verify_with_config,
};

const VEC_LABEL: &str = "pq-semaphore-d10";
const ITERATIONS: u32 = 16;

static SEMAPHORE_PROOF: &[u8] =
    include_bytes!("../../zkmcu-vectors/data/pq-semaphore-d10/proof.bin");
static SEMAPHORE_PUBLIC: &[u8] =
    include_bytes!("../../zkmcu-vectors/data/pq-semaphore-d10/public.bin");

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
        c"zkmcu: PQ-Semaphore reject-time benchmark (Hazard3 RV32)"
    ),
    hal::binary_info::rp_program_build_attribute!(),
];

#[hal::entry]
fn main() -> ! {
    // SAFETY: HEAP_MEM is a static [MaybeUninit<u8>] with a unique address;
    // HEAP.init is called exactly once before any allocation.
    unsafe { HEAP.init(core::ptr::addr_of_mut!(HEAP_MEM) as usize, HEAP_SIZE) }

    init_cycle_counter();
    let pac = hal::pac::Peripherals::take().expect("rp235x PAC once");
    let (timer, usb_bus) = init_rp2350(pac);

    let mut bench = Bench::new(
        &usb_bus,
        timer,
        BenchConfig {
            manufacturer: "zkmcu",
            product: "bench-rp2350-rv32-pq-semaphore-reject",
            serial: "0001",
        },
    );

    bench.enumerate_for(2_000_000);

    let mut boot_line: String<160> = String::new();
    let _ = writeln!(
        &mut boot_line,
        "zkmcu boot: heap=384K sys=150MHz core=hazard3 alloc=tlsf proof={VEC_LABEL} mode=reject patterns={} iters={ITERATIONS}\r",
        ALL.len(),
    );
    bench.write_line(boot_line.as_bytes());

    let sys_hz: u64 = u64::from(SYS_HZ);

    for mutation in ALL {
        run_pattern(&mut bench, sys_hz, mutation);
    }

    bench.write_line(b"[done] all patterns complete\r\n");
    loop {
        bench.pace(2_000_000);
    }
}

fn run_pattern<B: UsbBus>(bench: &mut Bench<'_, B>, sys_hz: u64, mutation: Mutation) {
    let name = mutation.name();
    let mut header: String<128> = String::new();
    let _ = writeln!(&mut header, "[pattern] {name} start\r");
    bench.write_line(header.as_bytes());

    for iter in 1..=ITERATIONS {
        HEAP.reset_peak();
        let heap_before = HEAP.current();

        let (result, cycles) = measure_cycles(|| run_one(mutation));
        let heap_peak = HEAP.peak();
        let us = cycles.saturating_mul(1_000_000) / sys_hz;
        let verdict = match (mutation, &result) {
            // Honest path must accept; everything else must reject.
            (Mutation::None, Ok(())) => "ok=true",
            (Mutation::None, Err(_)) => "ok=false",
            (_, Ok(())) => "ok=false_unexpected_accept",
            (_, Err(_)) => "ok=true_rejected",
        };

        let mut out: String<224> = String::new();
        let _ = writeln!(
            &mut out,
            "[{name} {iter}] cycles={cycles} us={us} ms={} heap_base={heap_before} heap_peak={heap_peak} {verdict}",
            us / 1000,
        );
        bench.write_line(out.as_bytes());
        bench.pace(200_000);
    }
}

fn run_one(mutation: Mutation) -> Result<(), VerifyOutcome> {
    let mut proof_bytes: Vec<u8> = SEMAPHORE_PROOF.to_vec();
    let mut public_bytes: Vec<u8> = SEMAPHORE_PUBLIC.to_vec();
    mutation.apply(&mut proof_bytes, &mut public_bytes);

    let Ok(proof) = parse_proof(&proof_bytes) else {
        return Err(VerifyOutcome::ProofParse);
    };
    drop(proof_bytes);
    let Ok(public) = parse_public_inputs(&public_bytes) else {
        return Err(VerifyOutcome::PublicParse);
    };
    drop(public_bytes);

    let config = make_config();
    let air = build_air();
    verify_with_config(&proof, &public, &config, &air).map_err(|_| VerifyOutcome::VerifyFailed)
}

#[derive(Debug)]
enum VerifyOutcome {
    ProofParse,
    PublicParse,
    VerifyFailed,
}
