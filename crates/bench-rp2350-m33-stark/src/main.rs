#![no_std]
#![no_main]
#![allow(clippy::integer_division)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::panic)]

use core::fmt::Write as _;
use core::mem::MaybeUninit;

use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicUsize, Ordering};
use cortex_m::peripheral::DWT;
use embedded_alloc::LlffHeap;
use heapless::String;
use panic_halt as _;
use rp235x_hal as hal;
use usb_device::class_prelude::*;
use usb_device::prelude::*;
use usbd_serial::SerialPort;
use zkmcu_verifier_stark::fibonacci::{self, PublicInputs};
use zkmcu_verifier_stark::{parse_proof, Proof};

type Timer0 = hal::Timer<hal::timer::CopyableTimer0>;

// Baked-in Fibonacci STARK proof — generated once by:
//   cargo run -p zkmcu-host-gen --release -- stark
// Lives at crates/zkmcu-vectors/data/stark-fib-1024/. No include via
// zkmcu-vectors function for now; phase 3.1 keeps it direct to mirror
// what the BLS12 firmware does.
static FIB_PROOF: &[u8] = include_bytes!("../../zkmcu-vectors/data/stark-fib-1024/proof.bin");
static FIB_PUBLIC: &[u8] = include_bytes!("../../zkmcu-vectors/data/stark-fib-1024/public.bin");

// TrackingHeap — identical to the BN254 / BLS12 firmware. Two relaxed
// atomic ops per alloc/dealloc, negligible next to the hashing + FRI
// work STARK verify does inside them.
struct TrackingHeap {
    inner: LlffHeap,
    current: AtomicUsize,
    peak: AtomicUsize,
}

impl TrackingHeap {
    const fn empty() -> Self {
        Self {
            inner: LlffHeap::empty(),
            current: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
        }
    }

    /// SAFETY: same contract as `LlffHeap::init` — call exactly once before
    /// any allocation happens, with a valid start address + size.
    unsafe fn init(&self, start: usize, size: usize) {
        // SAFETY: delegated under the same contract as this function.
        unsafe { self.inner.init(start, size) }
    }

    fn peak(&self) -> usize {
        self.peak.load(Ordering::Relaxed)
    }

    fn current(&self) -> usize {
        self.current.load(Ordering::Relaxed)
    }

    fn reset_peak(&self) {
        self.peak
            .store(self.current.load(Ordering::Relaxed), Ordering::Relaxed);
    }
}

// SAFETY: memory ops delegated to LlffHeap; atomic bookkeeping doesn't
// change ownership or aliasing.
unsafe impl GlobalAlloc for TrackingHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: delegated under the GlobalAlloc contract.
        let ptr = unsafe { self.inner.alloc(layout) };
        if !ptr.is_null() {
            let new = self.current.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            self.peak.fetch_max(new, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: delegated under the GlobalAlloc contract.
        unsafe { self.inner.dealloc(ptr, layout) }
        self.current.fetch_sub(layout.size(), Ordering::Relaxed);
    }
}

#[global_allocator]
static HEAP: TrackingHeap = TrackingHeap::empty();

// Prediction for STARK verify peak heap (see research/reports/
// 2026-04-23-stark-prediction.typ): 120-180 KB. Start oversized at
// 256 KB to de-risk first bring-up; the boot-time TrackingHeap readout
// will report actual peak, and a follow-up firmware build can tune
// this down once we have a measurement.
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
        c"zkmcu: STARK Fibonacci verify benchmark (Cortex-M33)"
    ),
    hal::binary_info::rp_program_build_attribute!(),
];

const XTAL_HZ: u32 = 12_000_000;
const SYS_HZ: u32 = 150_000_000;

#[hal::entry]
fn main() -> ! {
    // SAFETY: HEAP_MEM is a static [MaybeUninit<u8>] with a unique address;
    // HEAP.init is called exactly once before any allocation.
    unsafe { HEAP.init(core::ptr::addr_of_mut!(HEAP_MEM) as usize, HEAP_SIZE) }

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
            .product("bench-rp2350-m33-stark")
            .serial_number("0001")])
        .expect("USB strings")
        .max_packet_size_0(64)
        .expect("USB max packet size")
        .device_class(2)
        .build();

    // Pump USB polling for ~2 s so the host enumerates us.
    let enum_deadline = timer.get_counter().ticks() + 2_000_000;
    while timer.get_counter().ticks() < enum_deadline {
        usb_dev.poll(&mut [&mut serial]);
    }

    write_line(
        &mut usb_dev,
        &mut serial,
        timer,
        b"zkmcu boot: heap=256K sys=150MHz core=cortex-m33 proof=stark-fib-1024\r\n",
    );

    let sys_hz: u64 = u64::from(SYS_HZ);

    // Parse once at startup so the main loop times verify only (same
    // pattern as the BN254 and BLS12 firmware crates).
    let proof = parse_proof(FIB_PROOF).expect("parse stark proof");
    let public = fibonacci::parse_public(FIB_PUBLIC).expect("parse stark public");

    // One-shot boot measurement: peak stack + peak heap + cycles for one
    // verify. Parse cost is excluded; proof.clone() is hoisted out of the
    // timed window to isolate the allocator-jitter contribution from the
    // verify cost itself (phase 3.2.x variance-isolation experiment).
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
        // Clone happens OUTSIDE the timed window. winterfell::verify still
        // consumes the Proof, so we need a fresh clone each iteration —
        // but the allocator work lands outside the cycle-count span.
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

        // Pace the loop so we don't spam serial.
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
    HEAP.reset_peak();
    let heap_before = HEAP.current();

    let (verify_ok, stack_peak, cycles) = measure_verify_stack_peak(proof, public);

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
        "[boot] vec=stark-fib-1024 stack={stack_bytes} heap_base={heap_before} heap_peak={heap_peak} cycles={cycles} us={us} ms={} {verdict}",
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
