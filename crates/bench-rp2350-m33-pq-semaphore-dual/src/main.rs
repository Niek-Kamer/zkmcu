#![no_std]
#![no_main]
#![allow(clippy::integer_division)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::panic)]

use core::fmt::Write as _;
use core::mem::MaybeUninit;

use bench_core::{
    init_cycle_counter, init_rp2350, measure_cycles, measure_stack_peak, Bench, BenchConfig,
    TrackingTlsf, UsbBus, SYS_HZ,
};
use heapless::String;
use panic_halt as _;
use rp235x_hal as hal;
use zkmcu_verifier_plonky3::pq_semaphore_dual::parse_and_verify as parse_and_verify_dual;

const VEC_LABEL: &str = "pq-semaphore-d10-dual";

static DUAL_PROOF_P2: &[u8] =
    include_bytes!("../../zkmcu-vectors/data/pq-semaphore-d10-dual/proof_p2.bin");
static DUAL_PROOF_B3: &[u8] =
    include_bytes!("../../zkmcu-vectors/data/pq-semaphore-d10-dual/proof_b3.bin");
static DUAL_PUBLIC: &[u8] =
    include_bytes!("../../zkmcu-vectors/data/pq-semaphore-d10-dual/public.bin");

#[global_allocator]
static HEAP: TrackingTlsf = TrackingTlsf::empty();

// 384 KB heap. The dual entry point parses + verifies the Poseidon2 leg
// in a scoped block, drops it, then parses + verifies the Blake3 leg, so
// peak heap is `max(p2_peak, b3_peak)` rather than the sum. Phase B
// measured p2 peak at ~329 KB; the b3 leg is similarly shaped (slightly
// smaller wire bytes, similar parse expansion). 384 KB matches Phase B
// exactly so `measure_stack_peak`'s 64 KB sentinel still fits in the
// remaining stack region.
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
        c"zkmcu: Plonky3 PQ-Semaphore dual-hash (Poseidon2 + Blake3) verify (Cortex-M33)"
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
            product: "bench-rp2350-m33-pq-semaphore-dual",
            serial: "0001",
        },
    );

    bench.enumerate_for(2_000_000);

    let mut boot_line: String<128> = String::new();
    let _ = writeln!(
        &mut boot_line,
        "zkmcu boot: heap=384K sys=150MHz core=cortex-m33 alloc=tlsf proof={VEC_LABEL}\r",
    );
    bench.write_line(boot_line.as_bytes());

    let sys_hz: u64 = u64::from(SYS_HZ);

    boot_measure(&mut bench, sys_hz);

    let mut iter: u32 = 0;
    loop {
        iter = iter.wrapping_add(1);

        bench.print_marker(iter, b"pq_semaphore_dual_verify start\r\n");
        let (result, cycles) =
            measure_cycles(|| parse_and_verify_dual(DUAL_PROOF_P2, DUAL_PROOF_B3, DUAL_PUBLIC));
        let verdict = match result {
            Ok(()) => "ok=true",
            Err(_) => "ok=false",
        };
        let us = cycles.saturating_mul(1_000_000) / sys_hz;

        let mut out: String<192> = String::new();
        let _ = writeln!(
            &mut out,
            "[{iter}] pq_semaphore_dual_verify: cycles={cycles} us={us} ms={} heap_peak={} {verdict}",
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

fn boot_measure<B: UsbBus>(bench: &mut Bench<'_, B>, sys_hz: u64) {
    HEAP.reset_peak();
    let heap_before = HEAP.current();
    fence(bench, "boot_measure entered");

    let (verify_ok, stack_peak, cycles) =
        measure_stack_peak(|| parse_and_verify_dual(DUAL_PROOF_P2, DUAL_PROOF_B3, DUAL_PUBLIC));

    let heap_peak = HEAP.peak();
    let stack_bytes = stack_peak.unwrap_or(0);
    let us = cycles.saturating_mul(1_000_000) / sys_hz;
    let verdict = match verify_ok {
        Ok(()) => "ok=true",
        Err(_) => "ok=false",
    };
    let mut out: String<224> = String::new();
    let _ = writeln!(
        &mut out,
        "[boot] vec={VEC_LABEL} stack={stack_bytes} heap_base={heap_before} heap_peak={heap_peak} cycles={cycles} us={us} ms={} {verdict}",
        us / 1000
    );
    bench.write_line(out.as_bytes());
}
