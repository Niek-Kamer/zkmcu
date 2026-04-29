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
use zkmcu_verifier_plonky3::pq_semaphore::{
    build_air, make_config, parse_proof, parse_public_inputs, verify_with_config, Config,
    PqSemaphoreAir, Proof, Val, NUM_PUBLIC_INPUTS,
};

const VEC_LABEL: &str = "pq-semaphore-d10";

static SEMAPHORE_PROOF: &[u8] =
    include_bytes!("../../zkmcu-vectors/data/pq-semaphore-d10/proof.bin");
static SEMAPHORE_PUBLIC: &[u8] =
    include_bytes!("../../zkmcu-vectors/data/pq-semaphore-d10/public.bin");

#[global_allocator]
static HEAP: TrackingTlsf = TrackingTlsf::empty();

const HEAP_SIZE: usize = 256 * 1024;
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
        c"zkmcu: Plonky3 PQ-Semaphore depth-10 verify benchmark (Hazard3 RV32)"
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
            product: "bench-rp2350-rv32-pq-semaphore",
            serial: "0001",
        },
    );

    bench.enumerate_for(2_000_000);

    let mut boot_line: String<128> = String::new();
    let _ = writeln!(
        &mut boot_line,
        "zkmcu boot: heap=256K sys=150MHz core=hazard3 alloc=tlsf proof={VEC_LABEL}\r",
    );
    bench.write_line(boot_line.as_bytes());

    let sys_hz: u64 = u64::from(SYS_HZ);

    let proof = parse_proof(SEMAPHORE_PROOF).expect("parse pq-semaphore proof");
    let public = parse_public_inputs(SEMAPHORE_PUBLIC).expect("parse pq-semaphore public");
    let config = make_config();
    let air = build_air();

    boot_measure(&mut bench, sys_hz, &proof, &public, &config, &air);

    let mut iter: u32 = 0;
    loop {
        iter = iter.wrapping_add(1);

        bench.print_marker(iter, b"pq_semaphore_verify start\r\n");
        let (result, cycles) =
            measure_cycles(|| verify_with_config(&proof, &public, &config, &air));
        let verdict = match result {
            Ok(()) => "ok=true",
            Err(_) => "ok=false",
        };
        let us = cycles.saturating_mul(1_000_000) / sys_hz;

        let mut out: String<160> = String::new();
        let _ = writeln!(
            &mut out,
            "[{iter}] pq_semaphore_verify: cycles={cycles} us={us} ms={} {verdict}",
            us / 1000,
        );
        bench.write_line(out.as_bytes());

        bench.pace(1_000_000);
    }
}

fn boot_measure<B: UsbBus>(
    bench: &mut Bench<'_, B>,
    sys_hz: u64,
    proof: &Proof,
    public: &[Val; NUM_PUBLIC_INPUTS],
    config: &Config,
    air: &PqSemaphoreAir,
) {
    HEAP.reset_peak();
    let heap_before = HEAP.current();

    let (verify_ok, stack_peak, cycles) =
        measure_stack_peak(|| verify_with_config(proof, public, config, air));

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
