#![no_std]
#![no_main]
// Same justifications as bench-rp2350-m33: integer math, long init, init-panic safety.
#![allow(clippy::integer_division)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::panic)]

use core::fmt::Write as _;
use core::mem::MaybeUninit;

use bn::{pairing, Fr, Group, G1, G2};
use embedded_alloc::LlffHeap as Heap;
use heapless::String;
use panic_halt as _;
use rp235x_hal as hal;
use substrate_bn as bn;
use usb_device::class_prelude::*;
use usb_device::prelude::*;
use usbd_serial::SerialPort;

type Timer0 = hal::Timer<hal::timer::CopyableTimer0>;

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
    hal::binary_info::rp_program_description!(c"zkmcu: Groth16 verify benchmark (Hazard3 RV32)"),
    hal::binary_info::rp_program_build_attribute!(),
];

const XTAL_HZ: u32 = 12_000_000;
const SYS_HZ: u32 = 150_000_000;

/// Enable the `mcycle` counter.
///
/// Hazard3 boots with `mcountinhibit[CY] = 1`, which holds the cycle counter
/// at zero forever. We have to clear it explicitly before the counter becomes
/// useful. Writing 0 to `mcountinhibit` also ungates `minstret` and any other
/// HPM counters the core implements, which is what we want for benchmarking.
fn enable_mcycle() {
    // SAFETY: writing `mcountinhibit` in machine mode is always legal on
    // Hazard3 and has no side effects other than (un)inhibiting the HPM
    // counters. Bare-metal Rust on the RP2350 always runs in M-mode.
    unsafe {
        core::arch::asm!(
            "csrw mcountinhibit, zero",
            options(nomem, nostack, preserves_flags),
        );
    }
}

/// Read the 64-bit RISC-V machine cycle counter (`mcycle` / `mcycleh`).
///
/// The helper handles the high-word re-read in case `mcycle` wraps
/// between the two 32-bit CSR reads.
fn mcycle64() -> u64 {
    // SAFETY: reading the machine-mode cycle counter CSRs has no side effects
    // and no preconditions. Both CSRs are always available on Hazard3 in
    // machine mode, which is where bare-metal Rust runs.
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
    // SAFETY: `HEAP_MEM` is a static `[MaybeUninit<u8>]` with a unique address;
    // `HEAP.init` is called exactly once before any allocation happens.
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
            .product("bench-rp2350-rv32")
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
        b"zkmcu boot: heap=256K sys=150MHz core=hazard3\r\n",
    );

    let test_vector = zkmcu_vectors::square().expect("square test vector parse");
    let squares_5 = zkmcu_vectors::squares_5().expect("squares-5 test vector parse");
    let semaphore = zkmcu_vectors::semaphore_depth_10().expect("semaphore depth-10 parse");

    let sys_hz: u64 = u64::from(SYS_HZ);

    // One-shot stack + cycle measurements at boot for every test vector.
    // See M33 firmware for commentary.
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

        let mut seed = [0u8; 32];
        let seed_src = u64::from(iter).wrapping_add(mcycle64());
        seed[..8].copy_from_slice(&seed_src.to_le_bytes());
        seed[31] = 0;
        let s = Fr::from_slice(&seed).unwrap_or_else(|_| Fr::one());

        print_marker(&mut usb_dev, &mut serial, timer, iter, b"g1mul start\r\n");
        let t0 = mcycle64();
        let p = G1::one() * s;
        let t1 = mcycle64();
        let c_g1 = t1.wrapping_sub(t0);
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

        print_marker(&mut usb_dev, &mut serial, timer, iter, b"g2mul start\r\n");
        let t0 = mcycle64();
        let q = G2::one() * s;
        let t1 = mcycle64();
        let c_g2 = t1.wrapping_sub(t0);
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

        print_marker(&mut usb_dev, &mut serial, timer, iter, b"pairing start\r\n");
        let t0 = mcycle64();
        let gt = pairing(p, q);
        let t1 = mcycle64();
        let c_pair = t1.wrapping_sub(t0);
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

        print_marker(
            &mut usb_dev,
            &mut serial,
            timer,
            iter,
            b"groth16_verify start\r\n",
        );
        let t0 = mcycle64();
        let verify_result =
            zkmcu_verifier::verify(&test_vector.vk, &test_vector.proof, &test_vector.public);
        let t1 = mcycle64();
        let c_verify = t1.wrapping_sub(t0);
        let verify_label = match verify_result {
            Ok(true) => "ok=true",
            Ok(false) => "ok=false",
            Err(_) => "err",
        };
        let us = c_verify.saturating_mul(1_000_000) / sys_hz;
        {
            let mut out: String<160> = String::new();
            let _ = writeln!(
                &mut out,
                "[{iter}] groth16_verify: cycles={c_verify} us={us} ms={} {verify_label}",
                us / 1000,
            );
            write_line(&mut usb_dev, &mut serial, timer, out.as_bytes());
        }

        print_marker(
            &mut usb_dev,
            &mut serial,
            timer,
            iter,
            b"groth16_verify_semaphore start\r\n",
        );
        let t0 = mcycle64();
        let sem_result = zkmcu_verifier::verify(&semaphore.vk, &semaphore.proof, &semaphore.public);
        let t1 = mcycle64();
        let c_sem = t1.wrapping_sub(t0);
        let sem_label = match sem_result {
            Ok(true) => "ok=true",
            Ok(false) => "ok=false",
            Err(_) => "err",
        };
        let us_sem = c_sem.saturating_mul(1_000_000) / sys_hz;
        {
            let mut out: String<160> = String::new();
            let _ = writeln!(
                &mut out,
                "[{iter}] groth16_verify_semaphore: cycles={c_sem} us={us_sem} ms={} {sem_label}",
                us_sem / 1000,
            );
            write_line(&mut usb_dev, &mut serial, timer, out.as_bytes());
        }

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
    name: &str,
    ic_size: usize,
    public_len: usize,
    vk: &zkmcu_verifier::VerifyingKey,
    proof: &zkmcu_verifier::Proof,
    public: &[zkmcu_verifier::Fr],
) {
    let (verify_ok, stack_peak, cycles) = measure_verify_stack_peak(vk, proof, public);
    let bytes = stack_peak.unwrap_or(0);
    let us = cycles.saturating_mul(1_000_000) / sys_hz;
    let verdict = match verify_ok {
        Ok(true) => "ok=true",
        Ok(false) => "ok=false",
        Err(_) => "err",
    };
    let mut out: String<192> = String::new();
    let _ = writeln!(
        &mut out,
        "[boot] vec={name} ic={ic_size} public={public_len} stack={bytes} cycles={cycles} us={us} ms={} {verdict}",
        us / 1000
    );
    write_line(usb_dev, serial, timer, out.as_bytes());
}

// ---- Stack painting (see bench-rp2350-m33 for full commentary) ---------

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
    vk: &zkmcu_verifier::VerifyingKey,
    proof: &zkmcu_verifier::Proof,
    public: &[zkmcu_verifier::Fr],
) -> (Result<bool, zkmcu_verifier::Error>, Option<usize>, u64) {
    let sp = current_sp();
    let paint_top = (sp - STACK_PAINT_MARGIN) & !3usize;
    let paint_bottom = paint_top - STACK_PAINT_BYTES;

    let mut addr = paint_bottom;
    while addr < paint_top {
        // SAFETY: 4-byte-aligned address inside our own stack region, below the
        // current SP by at least STACK_PAINT_MARGIN bytes.
        #[allow(clippy::as_conversions)]
        unsafe {
            (addr as *mut u32).write_volatile(STACK_SENTINEL);
        }
        addr += 4;
    }

    let t0 = mcycle64();
    let result = zkmcu_verifier::verify(vk, proof, public);
    let t1 = mcycle64();
    let cycles = t1.wrapping_sub(t0);

    let mut addr = paint_bottom;
    while addr < paint_top {
        // SAFETY: reading the region we painted above.
        #[allow(clippy::as_conversions)]
        let val = unsafe { (addr as *const u32).read_volatile() };
        if val != STACK_SENTINEL {
            // See M33 firmware: add the unpainted margin back to the report.
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
