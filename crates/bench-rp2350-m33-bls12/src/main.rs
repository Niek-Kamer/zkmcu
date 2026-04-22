#![no_std]
#![no_main]
#![allow(clippy::integer_division)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::panic)]

use core::fmt::Write as _;
use core::mem::MaybeUninit;

use bls12_381::{pairing, G1Affine, G2Affine, Scalar};
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
use zkmcu_verifier_bls12 as zkvb;

type Timer0 = hal::Timer<hal::timer::CopyableTimer0>;

// Baked-in BLS12-381 test vectors. Direct `include_bytes!` from the
// zkmcu-vectors crate's data directory; zkmcu-vectors 0.1.0 only exposes the
// BN254 vectors as Rust functions, so we read the bytes directly here and
// parse with zkmcu-verifier-bls12.
static SQUARE_VK: &[u8] = include_bytes!("../../zkmcu-vectors/data/bls12-381/square/vk.bin");
static SQUARE_PROOF: &[u8] = include_bytes!("../../zkmcu-vectors/data/bls12-381/square/proof.bin");
static SQUARE_PUBLIC: &[u8] =
    include_bytes!("../../zkmcu-vectors/data/bls12-381/square/public.bin");

static SQUARES_5_VK: &[u8] = include_bytes!("../../zkmcu-vectors/data/bls12-381/squares-5/vk.bin");
static SQUARES_5_PROOF: &[u8] =
    include_bytes!("../../zkmcu-vectors/data/bls12-381/squares-5/proof.bin");
static SQUARES_5_PUBLIC: &[u8] =
    include_bytes!("../../zkmcu-vectors/data/bls12-381/squares-5/public.bin");

// TrackingHeap — identical to the BN254 firmware. Two relaxed atomic ops per
// alloc/dealloc, negligible next to the work the pairing library does inside.
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

    /// SAFETY: same contract as `LlffHeap::init` — call exactly once before any
    /// allocation.
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

// SAFETY: memory ops delegated to LlffHeap; atomic bookkeeping doesn't change
// ownership or aliasing.
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

// Prediction for BLS12-381 peak heap (see research/reports/
// 2026-04-22-bls12-381-prediction.typ): ~130 KB peak, ~160 KB arena with
// 17 % margin. Start oversized at 256 KB to de-risk first bring-up; the
// boot-time TrackingHeap readout will report actual peak, and a follow-up
// firmware build can tune this down once we have a measurement.
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
        c"zkmcu: Groth16/BLS12-381 verify benchmark (Cortex-M33)"
    ),
    hal::binary_info::rp_program_build_attribute!(),
];

const XTAL_HZ: u32 = 12_000_000;
const SYS_HZ: u32 = 150_000_000;

/// Parsed test vector: VK + proof + public inputs, pre-decoded at boot so the
/// verify loop doesn't pay parse cost or thrash the allocator.
struct TestVector {
    name: &'static str,
    vk: zkvb::VerifyingKey,
    proof: zkvb::Proof,
    public: alloc::vec::Vec<zkvb::Fr>,
}
extern crate alloc;

fn parse_vector(
    name: &'static str,
    vk_bytes: &[u8],
    proof_bytes: &[u8],
    public_bytes: &[u8],
) -> TestVector {
    let vk = zkvb::parse_vk(vk_bytes).expect("parse vk");
    let proof = zkvb::parse_proof(proof_bytes).expect("parse proof");
    let public = zkvb::parse_public(public_bytes).expect("parse public");
    TestVector {
        name,
        vk,
        proof,
        public,
    }
}

#[hal::entry]
fn main() -> ! {
    // SAFETY: `HEAP_MEM` is a static `[MaybeUninit<u8>]` with a unique address;
    // `HEAP.init` is called exactly once before any allocation.
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
            .product("bench-rp2350-m33-bls12")
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
        b"zkmcu boot: heap=256K sys=150MHz core=cortex-m33 curve=bls12-381\r\n",
    );

    let square = parse_vector("square", SQUARE_VK, SQUARE_PROOF, SQUARE_PUBLIC);
    let squares_5 = parse_vector("squares-5", SQUARES_5_VK, SQUARES_5_PROOF, SQUARES_5_PUBLIC);

    let sys_hz: u64 = u64::from(SYS_HZ);

    boot_measure(
        &mut usb_dev,
        &mut serial,
        timer,
        sys_hz,
        square.name,
        square.vk.ic.len(),
        square.public.len(),
        &square.vk,
        &square.proof,
        &square.public,
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

    let mut iter: u32 = 0;

    loop {
        iter = iter.wrapping_add(1);

        // Seed → Scalar. Top byte = 0 keeps the value < 2^248, well under the
        // BLS12-381 scalar modulus r. `from_bytes` expects little-endian, so the
        // 8-byte seed goes into the low bytes.
        let mut seed = [0u8; 32];
        let seed_src = u64::from(iter).wrapping_add(u64::from(DWT::cycle_count()));
        seed[..8].copy_from_slice(&seed_src.to_le_bytes());
        seed[31] = 0;
        let s: Scalar = Option::from(Scalar::from_bytes(&seed))
            .expect("seed < r by construction (top byte cleared)");

        // ---- G1 scalar mul ----
        print_marker(&mut usb_dev, &mut serial, timer, iter, b"g1mul start\r\n");
        let t0 = DWT::cycle_count();
        let p = G1Affine::generator() * s;
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
        let q = G2Affine::generator() * s;
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
        let p_aff = G1Affine::from(p);
        let q_aff = G2Affine::from(q);
        let t0 = DWT::cycle_count();
        let gt = pairing(&p_aff, &q_aff);
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

        // ---- Full Groth16 verify — the headline number ----
        loop_verify(
            &mut usb_dev,
            &mut serial,
            timer,
            iter,
            "groth16_verify",
            &square.vk,
            &square.proof,
            &square.public,
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

        // Pace the loop to avoid spamming serial.
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
    vk: &zkvb::VerifyingKey,
    proof: &zkvb::Proof,
    public: &[zkvb::Fr],
    sys_hz: u64,
) {
    let mut marker: String<64> = String::new();
    let _ = write!(&mut marker, "[{iter}] {label} start\r\n");
    write_line(usb_dev, serial, timer, marker.as_bytes());

    let t0 = DWT::cycle_count();
    let result = zkvb::verify(vk, proof, public);
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
    vk: &zkvb::VerifyingKey,
    proof: &zkvb::Proof,
    public: &[zkvb::Fr],
) {
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
    vk: &zkvb::VerifyingKey,
    proof: &zkvb::Proof,
    public: &[zkvb::Fr],
) -> (Result<bool, zkvb::Error>, Option<usize>, u64) {
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
    let result = zkvb::verify(vk, proof, public);
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
