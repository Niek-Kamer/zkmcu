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
use zkmcu_verifier_plonky3::pq_semaphore_dual_ct::{
    parse_b3_constant_time, parse_p2_constant_time, parse_public_constant_time,
    verify_b3_leg_constant_time, verify_p2_leg_constant_time,
};

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

// 384 KB heap. Sized so the verifier's ~304 KB peak fits with ~80 KB of
// slack. The mutated proof bytes (`Vec<u8>`, 169 KB) and the parsed
// p2 `Proof` are dropped *before* the b3 leg starts (see `run_pattern`),
// so peak is bounded by `max(parsed_p2 + p2_inner, parsed_b3 + b3_inner)`
// rather than `parsed_p2 + parsed_b3 + raw_bytes + verify_inner`.
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
        c"zkmcu: PQ-Semaphore CT reject-time benchmark (Cortex-M33)"
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
            product: "bench-rp2350-m33-pq-semaphore-ct-reject",
            serial: "0001",
        },
    );

    bench.enumerate_for(2_000_000);

    let mut boot_line: String<160> = String::new();
    let _ = writeln!(
        &mut boot_line,
        "zkmcu boot: heap=384K sys=150MHz core=cortex-m33 alloc=tlsf proof={VEC_LABEL} mode=ct-reject patterns={} iters={ITERATIONS}\r",
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
        // Untimed setup: copy bytes onto the heap, apply mutation,
        // parse to owned `Proof` + `Public`, then drop the raw bytes.
        // Dropping the 169 KB p2 `Vec` before measure_cycles starts is
        // what makes the bench fit in the 384 KB heap — verify peaks
        // at ~304 KB and the parsed-then-dropped bytes don't overlap.
        let mut p2_bytes: Vec<u8> = DUAL_PROOF_P2.to_vec();
        let mut public_bytes: Vec<u8> = DUAL_PUBLIC.to_vec();
        mutation.apply(&mut p2_bytes, &mut public_bytes);
        let (p2_parsed, p2_parse_ok) = parse_p2_constant_time(&p2_bytes);
        drop(p2_bytes);
        let (public_parsed, public_parse_ok) = parse_public_constant_time(&public_bytes);
        drop(public_bytes);

        HEAP.reset_peak();
        let heap_before = HEAP.current();

        // Timed: verify p2 leg, drop parsed p2, parse + verify b3 leg.
        // Bitwise AND across all `*_ok` bools so neither leg short-
        // circuits — preserves the macro-CT property of the one-stage
        // entry point.
        let (accepted, cycles) = measure_cycles(move || {
            let r1 = verify_p2_leg_constant_time(&p2_parsed, &public_parsed);
            drop(p2_parsed);
            let (b3_parsed, b3_parse_ok) = parse_b3_constant_time(DUAL_PROOF_B3);
            let r2 = verify_b3_leg_constant_time(&b3_parsed, &public_parsed);
            public_parse_ok & p2_parse_ok & r1 & b3_parse_ok & r2
        });

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
