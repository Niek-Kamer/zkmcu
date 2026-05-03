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
use zkmcu_verifier_plonky3::pq_semaphore_dual_ct::verify_constant_time;

const VEC_LABEL: &str = "pq-semaphore-d10-dual";
const ITERATIONS: u32 = 16;

static DUAL_PROOF_P2: &[u8] =
    include_bytes!("../../zkmcu-vectors/data/pq-semaphore-d10-dual/proof_p2.bin");
static DUAL_PROOF_B3: &[u8] =
    include_bytes!("../../zkmcu-vectors/data/pq-semaphore-d10-dual/proof_b3.bin");
static DUAL_PUBLIC: &[u8] =
    include_bytes!("../../zkmcu-vectors/data/pq-semaphore-d10-dual/public.bin");

#[global_allocator]
static HEAP: TrackingTlsf = TrackingTlsf::empty();

// 480 KB heap — see the M33 sibling for the sizing analysis. Vec-on-
// heap of the 172 KB mutated proof + 304 KB verify peak = 476 KB,
// fits with 4 KB slack; remaining 32 KB SRAM covers stack + USB.
const HEAP_SIZE: usize = 480 * 1024;
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
        c"zkmcu: PQ-Semaphore CT reject-time benchmark (Hazard3 RV32)"
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
            product: "bench-rp2350-rv32-pq-semaphore-ct-reject",
            serial: "0001",
        },
    );

    bench.enumerate_for(2_000_000);

    let mut boot_line: String<192> = String::new();
    let _ = writeln!(
        &mut boot_line,
        "zkmcu boot: heap=480K sys=150MHz core=hazard3 alloc=tlsf proof={VEC_LABEL} mode=ct-reject patterns={} iters={ITERATIONS}\r",
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

        let (accepted, cycles) = measure_cycles(|| run_one(mutation));
        let heap_peak = HEAP.peak();
        let us = cycles.saturating_mul(1_000_000) / sys_hz;
        let verdict = match (mutation, accepted) {
            (Mutation::None, true) => "ok=true",
            (Mutation::None, false) => "ok=false",
            (_, true) => "ok=false_unexpected_accept",
            (_, false) => "ok=true_rejected",
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

// Mirrors the single-leg `pq-semaphore-reject` mutation pattern: the
// mutation flips bytes in the Poseidon2-leg proof and/or the public
// blob; the Blake3 leg stays honest. The dual CT entry point still
// runs both legs to completion regardless.
fn run_one(mutation: Mutation) -> bool {
    let mut proof_p2: Vec<u8> = DUAL_PROOF_P2.to_vec();
    let mut public_bytes: Vec<u8> = DUAL_PUBLIC.to_vec();
    mutation.apply(&mut proof_p2, &mut public_bytes);

    verify_constant_time(&proof_p2, DUAL_PROOF_B3, &public_bytes)
}
