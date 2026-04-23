#![no_std]
#![no_main]
#![allow(clippy::integer_division)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::panic)]

use core::fmt::Write as _;
use core::mem::MaybeUninit;

use cortex_m::peripheral::DWT;
use heapless::String;
use panic_halt as _;
use rp235x_hal as hal;
use usb_device::class_prelude::*;
use usb_device::prelude::*;
use usbd_serial::SerialPort;
use zkmcu_bump_alloc::BumpAlloc;
use zkmcu_verifier_stark::fibonacci::{self, PublicInputs};
use zkmcu_verifier_stark::{parse_proof, Proof};

type Timer0 = hal::Timer<hal::timer::CopyableTimer0>;

static FIB_PROOF: &[u8] = include_bytes!("../../zkmcu-vectors/data/stark-fib-1024/proof.bin");
static FIB_PUBLIC: &[u8] = include_bytes!("../../zkmcu-vectors/data/stark-fib-1024/public.bin");

// Bump allocator with watermark save/restore — phase 3.2.x follow-up
// to the variance-isolation report. Between iterations we reset the
// bump pointer to a fixed checkpoint so that every verify starts with
// an identical allocator state. This should bring iteration-to-iteration
// variance close to the silicon baseline (~0.03–0.1 %) by eliminating
// the free-list-evolution source of jitter that LlffHeap exhibited.
#[global_allocator]
static HEAP: BumpAlloc = BumpAlloc::new();

// Arena sized up from 256 KB to 384 KB for the bump-alloc variant.
// BumpAlloc's realloc is in-place when the resized allocation is on
// top of the bump, but falls back to alloc-copy-leak otherwise — and
// winterfell's internal Vec growth isn't guaranteed to keep the
// growing Vec on top. 384 KB gives us headroom for the leaked slots
// and still leaves ~64 KB for stack + code + static data on the
// RP2350's 520 KB SRAM.
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
        c"zkmcu: STARK Fibonacci verify benchmark (Cortex-M33, bump-alloc)"
    ),
    hal::binary_info::rp_program_build_attribute!(),
];

const XTAL_HZ: u32 = 12_000_000;
const SYS_HZ: u32 = 150_000_000;

#[hal::entry]
fn main() -> ! {
    // SAFETY: HEAP_MEM is a static [MaybeUninit<u8>] with a unique address;
    // HEAP.init is called exactly once before any allocation happens.
    unsafe {
        HEAP.init(core::ptr::addr_of_mut!(HEAP_MEM).cast::<u8>(), HEAP_SIZE);
    }

    let mut cp = cortex_m::Peripherals::take().expect("cortex-m peripherals once");
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

    cp.DCB.enable_trace();
    cp.DWT.enable_cycle_counter();

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
            .product("bench-rp2350-m33-stark-bump")
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

    write_line(
        &mut usb_dev,
        &mut serial,
        timer,
        b"zkmcu boot: heap=384K sys=150MHz core=cortex-m33 alloc=bump proof=stark-fib-1024\r\n",
    );

    let sys_hz: u64 = u64::from(SYS_HZ);

    let proof = parse_proof(FIB_PROOF).expect("parse stark proof");
    let public = fibonacci::parse_public(FIB_PUBLIC).expect("parse stark public");

    // Checkpoint the bump pointer. Everything allocated after this
    // point (proof clones, winterfell internal Vecs, USB-side heapless
    // buffers that somehow escape the stack) gets reclaimed by
    // HEAP.reset_to(reset_point) between iterations.
    let reset_point = HEAP.watermark();

    boot_measure(
        &mut usb_dev,
        &mut serial,
        timer,
        sys_hz,
        proof.clone(),
        public,
        reset_point,
    );

    let mut iter: u32 = 0;
    loop {
        iter = iter.wrapping_add(1);

        // SAFETY: no live references to memory above `reset_point` exist
        // at this point. The previous iteration's proof clone was moved
        // into `verify` and dropped; the verify return value
        // (Result<(), Error>) owns no heap memory; heapless::String used
        // for the USB output line is stack-allocated. Any formatting
        // buffers from the previous iter's write_line have been
        // discarded by the writer returning.
        unsafe { HEAP.reset_to(reset_point) };

        print_marker(
            &mut usb_dev,
            &mut serial,
            timer,
            iter,
            b"stark_verify start\r\n",
        );
        // Clone sits outside the timed window (same as the phase 3.2.x
        // clone-hoisted run) so the cycle-count span reflects pure
        // verify work.
        let cloned = proof.clone();
        let t0 = DWT::cycle_count();
        let result = fibonacci::verify(cloned, public);
        let t1 = DWT::cycle_count();
        let cycles = u64::from(t1.wrapping_sub(t0));
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

#[allow(clippy::too_many_arguments)]
fn boot_measure<B: UsbBus>(
    usb_dev: &mut UsbDevice<'_, B>,
    serial: &mut SerialPort<'_, B>,
    timer: Timer0,
    sys_hz: u64,
    proof: Proof,
    public: PublicInputs,
    reset_point: usize,
) {
    let heap_base = HEAP.used_bytes();

    let (verify_ok, stack_peak, cycles) = measure_verify_stack_peak(proof, public);

    // heap_peak = bytes allocated since the reset_point checkpoint.
    // Under the bump allocator this is the exact high-water mark for
    // the verify call — bump alloc never frees mid-verify, so the
    // current used_bytes IS the peak.
    let heap_peak = HEAP
        .watermark()
        .saturating_sub(reset_point)
        .saturating_add(heap_base);
    let stack_bytes = stack_peak.unwrap_or(0);
    let us = cycles.saturating_mul(1_000_000) / sys_hz;
    let verdict = match verify_ok {
        Ok(()) => "ok=true",
        Err(_) => "ok=false",
    };
    let mut out: String<224> = String::new();
    let _ = writeln!(
        &mut out,
        "[boot] vec=stark-fib-1024 stack={stack_bytes} heap_base={heap_base} heap_peak={heap_peak} cycles={cycles} us={us} ms={} {verdict}",
        us / 1000
    );
    write_line(usb_dev, serial, timer, out.as_bytes());
}

// ---- Stack painting ----------------------------------------------------

const STACK_SENTINEL: u32 = 0xDEAD_BEEF;
const STACK_PAINT_BYTES: usize = 64 * 1024;
const STACK_PAINT_MARGIN: usize = 512;

fn current_sp() -> usize {
    let sp: usize;
    // SAFETY: reading the stack pointer has no side effects.
    unsafe {
        core::arch::asm!(
            "mov {sp}, sp",
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

    let t0 = DWT::cycle_count();
    let result = fibonacci::verify(proof, public);
    let t1 = DWT::cycle_count();
    let cycles = u64::from(t1.wrapping_sub(t0));

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
