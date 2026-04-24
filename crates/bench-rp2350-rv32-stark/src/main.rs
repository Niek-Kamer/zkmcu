#![no_std]
#![no_main]
#![allow(clippy::integer_division)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::panic)]

use core::fmt::Write as _;
use core::mem::MaybeUninit;

use embedded_alloc::TlsfHeap as Heap;
use heapless::String;
use panic_halt as _;
use rp235x_hal as hal;
use usb_device::class_prelude::*;
use usb_device::prelude::*;
use usbd_serial::SerialPort;
// Base-field selector. Default is Goldilocks + Quadratic (phase 3.2);
// `--features babybear` swaps to BabyBear + Quartic (phase 3.3).
#[cfg(not(feature = "babybear"))]
use zkmcu_verifier_stark::fibonacci as fib;
#[cfg(feature = "babybear")]
use zkmcu_verifier_stark::fibonacci_babybear as fib;

use fib::PublicInputs;
use zkmcu_verifier_stark::{parse_proof, Proof};

type Timer0 = hal::Timer<hal::timer::CopyableTimer0>;

#[cfg(not(feature = "babybear"))]
const VEC_LABEL: &str = "stark-fib-1024";
#[cfg(feature = "babybear")]
const VEC_LABEL: &str = "stark-fib-1024-babybear";

// Baked-in Fibonacci STARK proof. See bench-rp2350-m33-stark/src/main.rs
// for the rationale on direct include_bytes! (same pattern as BLS12).
#[cfg(not(feature = "babybear"))]
static FIB_PROOF: &[u8] = include_bytes!("../../zkmcu-vectors/data/stark-fib-1024/proof.bin");
#[cfg(not(feature = "babybear"))]
static FIB_PUBLIC: &[u8] = include_bytes!("../../zkmcu-vectors/data/stark-fib-1024/public.bin");

#[cfg(feature = "babybear")]
static FIB_PROOF: &[u8] =
    include_bytes!("../../zkmcu-vectors/data/stark-fib-1024-babybear/proof.bin");
#[cfg(feature = "babybear")]
static FIB_PUBLIC: &[u8] =
    include_bytes!("../../zkmcu-vectors/data/stark-fib-1024-babybear/public.bin");

// TlsfHeap, phase 3.2.z. O(1) two-level segregated fit. Pairs with
// the M33 TLSF firmware to measure whether TLSF gives bump-allocator-
// level variance (0.08 % IQR) while still supporting dealloc so heap
// peak stays in the 128 KB production tier.
#[global_allocator]
static HEAP: Heap = Heap::empty();

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
        c"zkmcu: STARK Fibonacci verify benchmark (Hazard3 RV32)"
    ),
    hal::binary_info::rp_program_build_attribute!(),
];

const XTAL_HZ: u32 = 12_000_000;
const SYS_HZ: u32 = 150_000_000;

/// Enable Hazard3's `mcycle` counter.
fn enable_mcycle() {
    // SAFETY: writing `mcountinhibit` in machine mode is always legal on
    // Hazard3 and has no side effects other than (un)inhibiting HPM counters.
    unsafe {
        core::arch::asm!(
            "csrw mcountinhibit, zero",
            options(nomem, nostack, preserves_flags),
        );
    }
}

/// Read the 64-bit `mcycle` counter.
fn mcycle64() -> u64 {
    // SAFETY: reading machine-mode CSRs has no side effects or preconditions
    // in M-mode, which is where bare-metal Rust runs on the RP2350.
    unsafe {
        let mut hi: u32;
        let mut lo: u32;
        let mut hi2: u32;
        loop {
            core::arch::asm!(
                "csrr {hi}, mcycleh",
                "csrr {lo}, mcycle",
                "csrr {hi2}, mcycleh",
                hi = out(reg) hi,
                lo = out(reg) lo,
                hi2 = out(reg) hi2,
                options(nomem, nostack, preserves_flags),
            );
            if hi == hi2 {
                return (u64::from(hi) << 32) | u64::from(lo);
            }
        }
    }
}

#[hal::entry]
fn main() -> ! {
    // SAFETY: HEAP_MEM is a static [MaybeUninit<u8>] with a unique address;
    // HEAP.init is called exactly once before any allocation.
    unsafe { HEAP.init(core::ptr::addr_of_mut!(HEAP_MEM) as usize, HEAP_SIZE) }

    enable_mcycle();

    let mut pac = hal::pac::Peripherals::take().expect("rp235x PAC once");
    let mut watchdog = hal::Watchdog::new(pac.WATCHDOG);

    let Ok(clocks) = hal::clocks::init_clocks_and_plls(
        XTAL_HZ,
        pac.XOSC,
        pac.CLOCKS,
        pac.PLL_SYS,
        pac.PLL_USB,
        &mut pac.RESETS,
        &mut watchdog,
    ) else {
        panic!("clock init");
    };

    let timer = hal::Timer::new_timer0(pac.TIMER0, &mut pac.RESETS, &clocks);

    let usb_bus = UsbBusAllocator::new(hal::usb::UsbBus::new(
        pac.USB,
        pac.USB_DPRAM,
        clocks.usb_clock,
        true,
        &mut pac.RESETS,
    ));

    let mut serial = SerialPort::new(&usb_bus);

    let mut usb_dev = UsbDeviceBuilder::new(&usb_bus, UsbVidPid(0x16c0, 0x27dd))
        .strings(&[StringDescriptors::default()
            .manufacturer("zkmcu")
            .product("bench-rp2350-rv32-stark")
            .serial_number("0001")])
        .expect("USB strings")
        .max_packet_size_0(64)
        .expect("USB max packet size")
        .device_class(2)
        .build();

    let enum_deadline = timer.get_counter().ticks() + 2_000_000;
    while timer.get_counter().ticks() < enum_deadline {
        usb_dev.poll(&mut [&mut serial]);
    }

    let mut boot_line: String<128> = String::new();
    let _ = writeln!(
        &mut boot_line,
        "zkmcu boot: heap=256K sys=150MHz core=hazard3 alloc=tlsf proof={VEC_LABEL}\r",
    );
    write_line(&mut usb_dev, &mut serial, timer, boot_line.as_bytes());

    let sys_hz: u64 = u64::from(SYS_HZ);

    let proof = parse_proof(FIB_PROOF).expect("parse stark proof");
    let public = fib::parse_public(FIB_PUBLIC).expect("parse stark public");

    boot_measure(
        &mut usb_dev,
        &mut serial,
        timer,
        sys_hz,
        proof.clone(),
        public,
    );

    let mut iter: u32 = 0;
    loop {
        iter = iter.wrapping_add(1);

        print_marker(
            &mut usb_dev,
            &mut serial,
            timer,
            iter,
            b"stark_verify start\r\n",
        );
        // Clone outside the timed window, same pattern as phase 3.2.x
        // and 3.2.y.
        let cloned = proof.clone();
        let t0 = mcycle64();
        let result = fib::verify(cloned, public);
        let t1 = mcycle64();
        let cycles = t1.wrapping_sub(t0);
        let verdict = match result {
            Ok(()) => "ok=true",
            Err(_) => "ok=false",
        };
        let us = cycles.saturating_mul(1_000_000) / sys_hz;

        let mut out: String<160> = String::new();
        let _ = writeln!(
            &mut out,
            "[{iter}] stark_verify: cycles={cycles} us={us} ms={} {verdict}",
            us / 1000,
        );
        write_line(&mut usb_dev, &mut serial, timer, out.as_bytes());

        let pace_deadline = timer.get_counter().ticks() + 1_000_000;
        while timer.get_counter().ticks() < pace_deadline {
            usb_dev.poll(&mut [&mut serial]);
        }
    }
}

fn advance(remaining: &[u8], n: usize) -> &[u8] {
    remaining.get(n..).unwrap_or_default()
}

fn boot_measure<B: UsbBus>(
    usb_dev: &mut UsbDevice<'_, B>,
    serial: &mut SerialPort<'_, B>,
    timer: Timer0,
    sys_hz: u64,
    proof: Proof,
    public: PublicInputs,
) {
    let (verify_ok, stack_peak, cycles) = measure_verify_stack_peak(proof, public);
    let stack_bytes = stack_peak.unwrap_or(0);
    let us = cycles.saturating_mul(1_000_000) / sys_hz;
    let verdict = match verify_ok {
        Ok(()) => "ok=true",
        Err(_) => "ok=false",
    };
    // heap_peak not measured on RV32, no TrackingHeap wrapper (same as
    // all prior RV32 STARK runs). Expect ~80-100 KB based on M33 TLSF
    // run once measured.
    let mut out: String<192> = String::new();
    let _ = writeln!(
        &mut out,
        "[boot] vec={VEC_LABEL} stack={stack_bytes} cycles={cycles} us={us} ms={} {verdict}",
        us / 1000
    );
    write_line(usb_dev, serial, timer, out.as_bytes());
}

// ---- Stack painting ---------------------------------------------------

const STACK_SENTINEL: u32 = 0xDEAD_BEEF;
const STACK_PAINT_BYTES: usize = 64 * 1024;
const STACK_PAINT_MARGIN: usize = 512;

fn current_sp() -> usize {
    let sp: usize;
    // SAFETY: reading the stack pointer has no side effects.
    unsafe {
        core::arch::asm!(
            "mv {sp}, sp",
            sp = out(reg) sp,
            options(nomem, nostack, preserves_flags),
        );
    }
    sp
}

#[inline(never)]
fn measure_verify_stack_peak(
    proof: Proof,
    public: PublicInputs,
) -> (Result<(), zkmcu_verifier_stark::Error>, Option<usize>, u64) {
    let sp = current_sp();
    let paint_top = (sp - STACK_PAINT_MARGIN) & !3usize;
    let paint_bottom = paint_top - STACK_PAINT_BYTES;

    let mut addr = paint_bottom;
    while addr < paint_top {
        // SAFETY: 4-byte-aligned address inside our own stack region, below
        // the current SP by at least STACK_PAINT_MARGIN bytes.
        #[allow(clippy::as_conversions)]
        unsafe {
            (addr as *mut u32).write_volatile(STACK_SENTINEL);
        }
        addr += 4;
    }

    let t0 = mcycle64();
    let result = fib::verify(proof, public);
    let t1 = mcycle64();
    let cycles = t1.wrapping_sub(t0);

    let mut addr = paint_bottom;
    while addr < paint_top {
        // SAFETY: reading the region we painted above.
        #[allow(clippy::as_conversions)]
        let val = unsafe { (addr as *const u32).read_volatile() };
        if val != STACK_SENTINEL {
            return (result, Some(paint_top - addr + STACK_PAINT_MARGIN), cycles);
        }
        addr += 4;
    }

    (result, None, cycles)
}

fn write_line<B: UsbBus>(
    usb_dev: &mut UsbDevice<'_, B>,
    serial: &mut SerialPort<'_, B>,
    timer: Timer0,
    data: &[u8],
) {
    let mut remaining = data;
    let deadline = timer.get_counter().ticks() + 1_000_000;
    while !remaining.is_empty() && timer.get_counter().ticks() < deadline {
        usb_dev.poll(&mut [serial]);
        match serial.write(remaining) {
            Ok(n) if n > 0 => remaining = advance(remaining, n),
            _ => {}
        }
    }
    let flush_deadline = timer.get_counter().ticks() + 20_000;
    while timer.get_counter().ticks() < flush_deadline {
        usb_dev.poll(&mut [serial]);
    }
}

fn print_marker<B: UsbBus>(
    usb_dev: &mut UsbDevice<'_, B>,
    serial: &mut SerialPort<'_, B>,
    timer: Timer0,
    iter: u32,
    tag: &[u8],
) {
    let mut out: String<64> = String::new();
    let _ = write!(&mut out, "[{iter}] ");
    write_line(usb_dev, serial, timer, out.as_bytes());
    write_line(usb_dev, serial, timer, tag);
}
