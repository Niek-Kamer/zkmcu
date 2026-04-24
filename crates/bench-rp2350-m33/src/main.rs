#![no_std]
#![no_main]
// Embedded firmware is all integer math on timer ticks and cycle counters.
// Floating point has no place here; silence the lint that insists otherwise.
#![allow(clippy::integer_division)]
// The main entry does all the hardware bring-up inline; splitting it up would
// fragment the init sequence without clarifying it.
#![allow(clippy::too_many_lines)]
// Unrecoverable init failures panic into panic_halt, wich is the whole
// point. Continuing with bad hardware state is strictly worse than halting.
#![allow(clippy::panic)]

use core::fmt::Write as _;
use core::mem::MaybeUninit;

use bn::{pairing, Fq, Fr, Group, G1, G2};
use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicUsize, Ordering};
use cortex_m::peripheral::DWT;
use embedded_alloc::LlffHeap;
use heapless::String;
use panic_halt as _;
use rp235x_hal as hal;
use substrate_bn as bn;
use usb_device::class_prelude::*;
use usb_device::prelude::*;
use usbd_serial::SerialPort;

type Timer0 = hal::Timer<hal::timer::CopyableTimer0>;

/// A `GlobalAlloc` wrapper around `embedded_alloc::LlffHeap` that records peak
/// simultaneous allocation. Added for heap-peak benchmarking because the raw
/// `LlffHeap` doesn't expose peak-tracking, only current usage. Overhead is
/// two relaxed atomic ops per alloc/dealloc, negligible compared to the work
/// `substrate-bn` does inside those allocations.
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

    /// SAFETY: same contract as `LlffHeap::init`, call exactly once before any
    /// allocation; the `[start, start + size)` region must be valid, aligned,
    /// and owned exclusively by the heap for the duration of the program.
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

// SAFETY: all memory ops delegated to LlffHeap; we only add atomic bookkeeping
// around successful alloc/dealloc. No aliasing or ownership rules change.
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

// Measured peak heap usage during one verify: ~81.3 KB (see
// benchmarks/runs/2026-04-22-m33-heap-peak/). A 96 KB arena gives ~18 %
// margin above peak, enough to absorb allocator fragmentation without being
// generous. 96 KB + ~16 KB stack + ~1 KB statics ≈ 113 KB of RAM in use, so
// this build fits comfortably on any 128 KB SRAM-class MCU or secure element.
const HEAP_SIZE: usize = 96 * 1024;
static mut HEAP_MEM: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];

#[link_section = ".start_block"]
#[used]
pub static IMAGE_DEF: hal::block::ImageDef = hal::block::ImageDef::secure_exe();

#[link_section = ".bi_entries"]
#[used]
pub static PICOTOOL_ENTRIES: [hal::binary_info::EntryAddr; 4] = [
    hal::binary_info::rp_cargo_bin_name!(),
    hal::binary_info::rp_cargo_version!(),
    hal::binary_info::rp_program_description!(c"zkmcu: Groth16 verify benchmark (Cortex-M33)"),
    hal::binary_info::rp_program_build_attribute!(),
];

const XTAL_HZ: u32 = 12_000_000;
const SYS_HZ: u32 = 150_000_000;

/// Copy the `.ram_text` section from its flash LMA to its RAM VMA before
/// the normal cortex-m-rt startup (bss zeroing, data copy, main) runs.
/// Opt-in for code that should execute from SRAM rather than XIP flash;
/// currently the only consumer is `mul_reduce_armv8m` in the substrate-bn
/// fork.
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
    // SAFETY: the linker places .ram_text's image in flash starting at
    // __ram_text_lma_start, and reserves the corresponding RAM window
    // between __ram_text_vma_start and __ram_text_vma_end. Both are u32-
    // aligned by the ALIGN(4) directives in memory.x. pre_init runs before
    // any other code, so nothing has read from the destination window yet.
    unsafe {
        core::ptr::copy_nonoverlapping(src, dst, count);
    }
}

#[hal::entry]
fn main() -> ! {
    // SAFETY: `HEAP_MEM` is a static `[MaybeUninit<u8>]` with a unique address;
    // `HEAP.init` is called exactly once before any allocation happens.
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
            .product("bench-rp2350-m33")
            .serial_number("0001")])
        .expect("USB strings")
        .max_packet_size_0(64)
        .expect("USB max packet size")
        .device_class(2)
        .build();

    // Pump USB polling for ~2s so the host enumerates us and a reader can attach.
    let enum_deadline = timer.get_counter().ticks() + 2_000_000;
    while timer.get_counter().ticks() < enum_deadline {
        usb_dev.poll(&mut [&mut serial]);
    }

    write_line(
        &mut usb_dev,
        &mut serial,
        timer,
        b"zkmcu boot: heap=96K sys=150MHz core=cortex-m33\r\n",
    );

    // Self-test the UMAAL asm before any measurement. If the asm miscomputes
    // `Fq::mul`, any verify number we emit below would be meaningless, so
    // halt instead of publishing a corrupted baseline.
    if !run_umaal_kat(&mut usb_dev, &mut serial, timer) {
        loop {
            usb_dev.poll(&mut [&mut serial]);
        }
    }

    // Parse the test vector once at startup so we don't time the parse or thrash the heap.
    let test_vector = zkmcu_vectors::square().expect("square test vector parse");
    let squares_5 = zkmcu_vectors::squares_5().expect("squares-5 test vector parse");
    let semaphore = zkmcu_vectors::semaphore_depth_10().expect("semaphore depth-10 parse");

    let sys_hz: u64 = u64::from(SYS_HZ);

    // One-shot stack + cycle + heap measurement at boot, for every test vector.
    // Each row: peak stack, peak heap, verify latency for one circuit size.
    // The pairs across rows give the scaling of verify cost with public
    // inputs without needing to tear down the main loop.
    boot_measure(
        &mut usb_dev,
        &mut serial,
        timer,
        sys_hz,
        test_vector.name,
        test_vector.vk.ic.len(),
        test_vector.public.len(),
        &test_vector.vk,
        &test_vector.proof,
        &test_vector.public,
    );
    boot_measure(
        &mut usb_dev,
        &mut serial,
        timer,
        sys_hz,
        squares_5.name,
        squares_5.vk.ic.len(),
        squares_5.public.len(),
        &squares_5.vk,
        &squares_5.proof,
        &squares_5.public,
    );
    boot_measure(
        &mut usb_dev,
        &mut serial,
        timer,
        sys_hz,
        semaphore.name,
        semaphore.vk.ic.len(),
        semaphore.public.len(),
        &semaphore.vk,
        &semaphore.proof,
        &semaphore.public,
    );
    let mut iter: u32 = 0;

    loop {
        iter = iter.wrapping_add(1);

        // ---- seed scalar (non-constant so the compiler can't fold anything) ----
        let mut seed = [0u8; 32];
        let seed_src = u64::from(iter).wrapping_add(u64::from(DWT::cycle_count()));
        seed[..8].copy_from_slice(&seed_src.to_le_bytes());
        // Clear the top byte so the value is < field modulus.
        seed[31] = 0;
        let s = Fr::from_slice(&seed).unwrap_or_else(|_| Fr::one());

        // ---- G1 scalar mul ----
        print_marker(&mut usb_dev, &mut serial, timer, iter, b"g1mul start\r\n");
        let t0 = DWT::cycle_count();
        let p = G1::one() * s;
        let t1 = DWT::cycle_count();
        let c_g1 = u64::from(t1.wrapping_sub(t0));
        core::hint::black_box(&p);
        print_result(
            &mut usb_dev,
            &mut serial,
            timer,
            iter,
            "g1mul",
            c_g1,
            sys_hz,
        );

        // ---- G2 scalar mul ----
        print_marker(&mut usb_dev, &mut serial, timer, iter, b"g2mul start\r\n");
        let t0 = DWT::cycle_count();
        let q = G2::one() * s;
        let t1 = DWT::cycle_count();
        let c_g2 = u64::from(t1.wrapping_sub(t0));
        core::hint::black_box(&q);
        print_result(
            &mut usb_dev,
            &mut serial,
            timer,
            iter,
            "g2mul",
            c_g2,
            sys_hz,
        );

        // ---- Pairing ----
        print_marker(&mut usb_dev, &mut serial, timer, iter, b"pairing start\r\n");
        let t0 = DWT::cycle_count();
        let gt = pairing(p, q);
        let t1 = DWT::cycle_count();
        let c_pair = u64::from(t1.wrapping_sub(t0));
        core::hint::black_box(&gt);
        print_result(
            &mut usb_dev,
            &mut serial,
            timer,
            iter,
            "pairing",
            c_pair,
            sys_hz,
        );

        // ---- Full Groth16 verify (the headline number) ----
        loop_verify(
            &mut usb_dev,
            &mut serial,
            timer,
            iter,
            "groth16_verify",
            &test_vector.vk,
            &test_vector.proof,
            &test_vector.public,
            sys_hz,
        );

        // ---- Scaling data point: 5-public-input verify ----
        loop_verify(
            &mut usb_dev,
            &mut serial,
            timer,
            iter,
            "groth16_verify_sq5",
            &squares_5.vk,
            &squares_5.proof,
            &squares_5.public,
            sys_hz,
        );

        // ---- Real-world data point: Semaphore depth-10 verify ----
        loop_verify(
            &mut usb_dev,
            &mut serial,
            timer,
            iter,
            "groth16_verify_semaphore",
            &semaphore.vk,
            &semaphore.proof,
            &semaphore.public,
            sys_hz,
        );

        // Pace the loop so we don't spam.
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
fn loop_verify<B: UsbBus>(
    usb_dev: &mut UsbDevice<'_, B>,
    serial: &mut SerialPort<'_, B>,
    timer: Timer0,
    iter: u32,
    label: &str,
    vk: &zkmcu_verifier::VerifyingKey,
    proof: &zkmcu_verifier::Proof,
    public: &[zkmcu_verifier::Fr],
    sys_hz: u64,
) {
    let mut marker: String<64> = String::new();
    let _ = write!(&mut marker, "[{iter}] {label} start\r\n");
    write_line(usb_dev, serial, timer, marker.as_bytes());

    let t0 = DWT::cycle_count();
    let result = zkmcu_verifier::verify(vk, proof, public);
    let t1 = DWT::cycle_count();
    let cycles = u64::from(t1.wrapping_sub(t0));
    let verdict = match result {
        Ok(true) => "ok=true",
        Ok(false) => "ok=false",
        Err(_) => "err",
    };
    let us = cycles.saturating_mul(1_000_000) / sys_hz;

    let mut out: String<160> = String::new();
    let _ = writeln!(
        &mut out,
        "[{iter}] {label}: cycles={cycles} us={us} ms={} {verdict}",
        us / 1000,
    );
    write_line(usb_dev, serial, timer, out.as_bytes());
}

#[allow(clippy::too_many_arguments)]
fn boot_measure<B: UsbBus>(
    usb_dev: &mut UsbDevice<'_, B>,
    serial: &mut SerialPort<'_, B>,
    timer: Timer0,
    sys_hz: u64,
    name: &str,
    ic_size: usize,
    public_len: usize,
    vk: &zkmcu_verifier::VerifyingKey,
    proof: &zkmcu_verifier::Proof,
    public: &[zkmcu_verifier::Fr],
) {
    // Reset peak-heap tracking right before the measured call, so the reported
    // figure is the peak during this one verify and not cumulative from earlier
    // parses / setup.
    HEAP.reset_peak();
    let heap_before = HEAP.current();

    let (verify_ok, stack_peak, cycles) = measure_verify_stack_peak(vk, proof, public);

    let heap_peak = HEAP.peak();
    let stack_bytes = stack_peak.unwrap_or(0);
    let us = cycles.saturating_mul(1_000_000) / sys_hz;
    let verdict = match verify_ok {
        Ok(true) => "ok=true",
        Ok(false) => "ok=false",
        Err(_) => "err",
    };
    let mut out: String<224> = String::new();
    let _ = writeln!(
        &mut out,
        "[boot] vec={name} ic={ic_size} public={public_len} stack={stack_bytes} heap_base={heap_before} heap_peak={heap_peak} cycles={cycles} us={us} ms={} {verdict}",
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
    // SAFETY: reading the stack pointer has no side effects and produces a
    // valid address owned by the current execution context.
    unsafe {
        core::arch::asm!(
            "mov {sp}, sp",
            sp = out(reg) sp,
            options(nomem, nostack, preserves_flags),
        );
    }
    sp
}

/// Paint a window just below the current SP, call verify, then scan the
/// window to find the peak stack depth reached by verify. Also returns the
/// cycle count consumed by the verify call so we get both numbers from a
/// single flash cycle.
#[inline(never)]
fn measure_verify_stack_peak(
    vk: &zkmcu_verifier::VerifyingKey,
    proof: &zkmcu_verifier::Proof,
    public: &[zkmcu_verifier::Fr],
) -> (Result<bool, zkmcu_verifier::Error>, Option<usize>, u64) {
    let sp = current_sp();
    let paint_top = (sp - STACK_PAINT_MARGIN) & !3usize;
    let paint_bottom = paint_top - STACK_PAINT_BYTES;

    let mut addr = paint_bottom;
    while addr < paint_top {
        // SAFETY: `addr` is a 4-byte-aligned address strictly below the current
        // SP by at least STACK_PAINT_MARGIN bytes. The region is inside the
        // stack allocation reserved by the linker, owned exclusively by this
        // execution context, and is not backed by live frames.
        #[allow(clippy::as_conversions)]
        unsafe {
            (addr as *mut u32).write_volatile(STACK_SENTINEL);
        }
        addr += 4;
    }

    let t0 = DWT::cycle_count();
    let result = zkmcu_verifier::verify(vk, proof, public);
    let t1 = DWT::cycle_count();
    let cycles = u64::from(t1.wrapping_sub(t0));

    let mut addr = paint_bottom;
    while addr < paint_top {
        // SAFETY: we're reading the same region we just painted, which is
        // inside our own stack allocation.
        #[allow(clippy::as_conversions)]
        let val = unsafe { (addr as *const u32).read_volatile() };
        if val != STACK_SENTINEL {
            // Add the margin back, those bytes are part of verify's frame
            // chain but we deliberately didn't paint them to avoid clobbering
            // the measuring function's own frame.
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
    // Drain the TX FIFO a bit.
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

fn print_result<B: UsbBus>(
    usb_dev: &mut UsbDevice<'_, B>,
    serial: &mut SerialPort<'_, B>,
    timer: Timer0,
    iter: u32,
    label: &str,
    cycles: u64,
    sys_hz: u64,
) {
    let us = cycles.saturating_mul(1_000_000) / sys_hz;
    let mut out: String<128> = String::new();
    let _ = writeln!(
        &mut out,
        "[{iter}] {label}: cycles={cycles} us={us} ms={}",
        us / 1000,
    );
    write_line(usb_dev, serial, timer, out.as_bytes());
}

/// Pre-benchmark self-test: run the UMAAL KAT vectors through `Fq::mul`.
///
/// On this firmware `substrate-bn` is built with `cortex-m33-asm`, so every
/// `Fq::mul` dispatches through the hand-written ARMv8-M UMAAL assembly in
/// `mul_reduce_armv8m`. The committed fixture bytes were produced on host
/// by the same library without the asm feature (pure Rust `mul_reduce_rust`).
/// Byte-identical output across both paths over 256 random limb patterns is
/// strong evidence the asm agrees with the Rust reference on this silicon +
/// toolchain combination. A miscompute halts before any benchmark number is
/// printed, so a corrupted asm can't silently influence the headline figure.
///
/// Returns true on all-pass, false on first miscompute (with details already
/// emitted to serial).
fn run_umaal_kat<B: UsbBus>(
    usb_dev: &mut UsbDevice<'_, B>,
    serial: &mut SerialPort<'_, B>,
    timer: Timer0,
) -> bool {
    let bytes = zkmcu_vectors::UMAAL_KAT;
    let rec = zkmcu_vectors::UMAAL_KAT_RECORD_SIZE;
    let n = bytes.len() / rec;

    let t0 = DWT::cycle_count();

    for i in 0..n {
        let base = i * rec;
        let a_bytes = bytes.get(base..base + 32).expect("KAT a in range");
        let b_bytes = bytes.get(base + 32..base + 64).expect("KAT b in range");
        let expected = bytes
            .get(base + 64..base + 96)
            .expect("KAT product in range");

        let a = Fq::from_slice(a_bytes).expect("KAT a parses as Fq");
        let b = Fq::from_slice(b_bytes).expect("KAT b parses as Fq");
        let got = a * b;

        let mut got_bytes = [0u8; 32];
        got.to_big_endian(&mut got_bytes)
            .expect("Fq serialise into 32 bytes");

        if got_bytes.as_slice() != expected {
            let mut out: String<128> = String::new();
            let _ = writeln!(
                &mut out,
                "UMAAL KAT: FAIL at record {i} of {n}, asm diverges from Rust reference"
            );
            write_line(usb_dev, serial, timer, out.as_bytes());
            return false;
        }
    }

    let t1 = DWT::cycle_count();
    let cycles = u64::from(t1.wrapping_sub(t0));
    let us = cycles.saturating_mul(1_000_000) / u64::from(SYS_HZ);
    let mut out: String<128> = String::new();
    let _ = writeln!(
        &mut out,
        "UMAAL KAT: {n}/{n} OK ({cycles} cycles, {us} us, asm agrees with Rust reference)"
    );
    write_line(usb_dev, serial, timer, out.as_bytes());
    true
}
