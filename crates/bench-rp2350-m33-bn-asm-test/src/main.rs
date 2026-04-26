//! On-device cross-check: UMAAL asm `mul_reduce` vs portable `mul_reduce_u32_ref`.
//!
//! Runs 10 000 random Fq and Fr input pairs per iteration, compares ASM output
//! against the reference, and prints pass/fail + timing over USB-CDC.

#![no_std]
#![no_main]
#![allow(clippy::panic)]
#![allow(clippy::integer_division)]
#![allow(clippy::similar_names)] // fq_*/fr_* pairs are intentionally parallel

use core::fmt::Write as _;
use core::mem::MaybeUninit;

use bench_core::{
    init_cycle_counter, init_rp2350, measure_cycles, Bench, BenchConfig, TrackingTlsf, UsbBus,
    SYS_HZ,
};
use heapless::String;
use panic_halt as _;
use rp235x_hal as hal;

#[global_allocator]
static HEAP: TrackingTlsf = TrackingTlsf::empty();

// substrate-bn declares `extern crate alloc`; a small static heap satisfies
// the linker even though the selftest path never calls alloc.
const HEAP_SIZE: usize = 16 * 1024;
static mut HEAP_MEM: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];

#[link_section = ".start_block"]
#[used]
pub static IMAGE_DEF: hal::block::ImageDef = hal::block::ImageDef::secure_exe();

#[link_section = ".bi_entries"]
#[used]
pub static PICOTOOL_ENTRIES: [hal::binary_info::EntryAddr; 4] = [
    hal::binary_info::rp_cargo_bin_name!(),
    hal::binary_info::rp_cargo_version!(),
    hal::binary_info::rp_program_description!(c"zkmcu: BN254 ASM selftest (Cortex-M33)"),
    hal::binary_info::rp_program_build_attribute!(),
];

const ITERS: u32 = 10_000;

/// Copy the `.ram_text` section from its flash LMA to its RAM VMA.
/// `mul_reduce_armv8m` lives there; without this copy it executes from
/// uninitialised RAM and the device silently hangs or hard-faults.
#[cortex_m_rt::pre_init]
unsafe fn copy_ram_text() {
    extern "C" {
        static __ram_text_lma_start: u32;
        static mut __ram_text_vma_start: u32;
        static mut __ram_text_vma_end: u32;
    }
    let src = core::ptr::addr_of!(__ram_text_lma_start);
    let dst = core::ptr::addr_of_mut!(__ram_text_vma_start);
    let end = core::ptr::addr_of_mut!(__ram_text_vma_end);
    let count = (end as usize).wrapping_sub(dst as usize) / 4;
    // SAFETY: linker-guaranteed alignment and non-overlapping flash/RAM regions.
    unsafe {
        core::ptr::copy_nonoverlapping(src, dst, count);
    }
}

fn run_and_report<B: UsbBus>(bench: &mut Bench<'_, B>, iter: u32, sys_hz: u64) {
    let seed_fq = 0x1111_1111_u64.wrapping_mul(u64::from(iter));
    let seed_fr = 0x2222_2222_u64.wrapping_mul(u64::from(iter));

    let (fq_pass, fq_cycles) =
        measure_cycles(|| substrate_bn::arith::selftest_fq(seed_fq, ITERS));
    let fq_us = fq_cycles.saturating_mul(1_000_000) / sys_hz;

    let (fr_pass, fr_cycles) =
        measure_cycles(|| substrate_bn::arith::selftest_fr(seed_fr, ITERS));
    let fr_us = fr_cycles.saturating_mul(1_000_000) / sys_hz;

    let mut out: String<192> = String::new();
    let _ = writeln!(
        &mut out,
        "[{iter}] fq={} fq_us={fq_us} fr={} fr_us={fr_us}\r",
        if fq_pass { "pass" } else { "FAIL" },
        if fr_pass { "pass" } else { "FAIL" },
    );
    bench.write_line(out.as_bytes());
}

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
            product: "bench-rp2350-m33-bn-asm-test",
            serial: "0001",
        },
    );

    bench.enumerate_for(2_000_000);

    let mut boot_line: String<128> = String::new();
    let _ = writeln!(
        &mut boot_line,
        "zkmcu boot: bn-asm-test core=cortex-m33 iters={ITERS}\r",
    );
    bench.write_line(boot_line.as_bytes());

    let sys_hz: u64 = u64::from(SYS_HZ);

    let mut iter: u32 = 0;
    loop {
        iter = iter.wrapping_add(1);
        bench.pace(200_000);
        run_and_report(&mut bench, iter, sys_hz);
        bench.pace(2_000_000);
    }
}
