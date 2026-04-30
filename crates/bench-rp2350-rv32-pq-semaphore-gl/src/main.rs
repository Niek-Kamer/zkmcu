#![no_std]
#![no_main]
#![allow(clippy::integer_division)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::panic)]

use core::fmt::Write as _;
use core::mem::MaybeUninit;

use bench_core::{
    init_cycle_counter, init_rp2350, measure_cycles, Bench, BenchConfig, TrackingTlsf, UsbBus,
    SYS_HZ,
};
use heapless::String;
use panic_halt as _;
use rp235x_hal as hal;
use zkmcu_verifier_plonky3::pq_semaphore_goldilocks::{
    build_air, make_config, parse_proof, parse_public_inputs, verify_with_config,
};

const VEC_LABEL: &str = "pq-semaphore-d10-gl";

static SEMAPHORE_PROOF: &[u8] =
    include_bytes!("../../zkmcu-vectors/data/pq-semaphore-d10-gl/proof.bin");
static SEMAPHORE_PUBLIC: &[u8] =
    include_bytes!("../../zkmcu-vectors/data/pq-semaphore-d10-gl/public.bin");

#[global_allocator]
static HEAP: TrackingTlsf = TrackingTlsf::empty();

// 480 KB heap. Goldilocks proofs run ~265 KB on the wire and the
// parsed `Proof<Config>` lands at ~1.9× wire size in heap (per the
// BabyBear-d6 expansion ratio measured in
// benchmarks/runs/2026-04-29-m33-pq-semaphore-d6/), so peak heap is
// roughly 270 KB Proof + ~100 KB verify scratch ≈ 370 KB. The 384 KB
// heap from the BabyBear sibling would be cutting it close — 480 KB
// keeps us well clear of OOM. Pico 2 W has 524 KB SRAM, leaving ~32 KB
// for stack + static after this allocation.
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
        c"zkmcu: Plonky3 PQ-Semaphore Goldilocks-Quadratic verify (Hazard3 RV32)"
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
            product: "bench-rp2350-rv32-pq-semaphore-gl",
            serial: "0001",
        },
    );

    bench.enumerate_for(2_000_000);

    let mut boot_line: String<128> = String::new();
    let _ = writeln!(
        &mut boot_line,
        "zkmcu boot: heap=480K sys=150MHz core=hazard3 alloc=tlsf proof={VEC_LABEL}\r",
    );
    bench.write_line(boot_line.as_bytes());

    let sys_hz: u64 = u64::from(SYS_HZ);

    fence(&mut bench, "before parse_proof");
    let proof = parse_proof(SEMAPHORE_PROOF).expect("parse pq-semaphore-gl proof");
    fence(&mut bench, "after parse_proof");
    let public = parse_public_inputs(SEMAPHORE_PUBLIC).expect("parse pq-semaphore-gl public");
    fence(&mut bench, "after parse_public_inputs");
    let config = make_config();
    fence(&mut bench, "after make_config");
    let air = build_air();
    fence(&mut bench, "after build_air");

    // Skip stack-paint boot measurement: same reason as the M33 sibling —
    // the 480 KB heap leaves only ~32 KB of stack region, but
    // `measure_stack_peak` paints 64 KB below SP, which corrupts the
    // heap. Heap peak is captured below from the regular loop instead.

    let mut iter: u32 = 0;
    loop {
        iter = iter.wrapping_add(1);

        bench.print_marker(iter, b"pq_semaphore_gl_verify start\r\n");
        let (result, cycles) =
            measure_cycles(|| verify_with_config(&proof, &public, &config, &air));
        let verdict = match result {
            Ok(()) => "ok=true",
            Err(_) => "ok=false",
        };
        let us = cycles.saturating_mul(1_000_000) / sys_hz;

        let mut out: String<192> = String::new();
        let _ = writeln!(
            &mut out,
            "[{iter}] pq_semaphore_gl_verify: cycles={cycles} us={us} ms={} heap_peak={} {verdict}",
            us / 1000,
            HEAP.peak(),
        );
        bench.write_line(out.as_bytes());

        bench.pace(1_000_000);
    }
}

fn fence<B: UsbBus>(bench: &mut Bench<'_, B>, where_: &str) {
    let mut out: String<96> = String::new();
    let _ = writeln!(
        &mut out,
        "[fp] {where_} heap_cur={} heap_peak={}\r",
        HEAP.current(),
        HEAP.peak(),
    );
    bench.write_line(out.as_bytes());
}
