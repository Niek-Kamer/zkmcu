#![no_std]
#![no_main]
#![allow(clippy::integer_division)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::panic)]

use core::mem::MaybeUninit;

use bench_core::{init_cycle_counter, init_rp2350, measure_cycles, Bench, BenchConfig, UsbBus, SYS_HZ};
use heapless::String;
use core::fmt::Write as _;
use panic_halt as _;
use rp235x_hal as hal;

// zkmcu-verifier allocates heap for VK IC table. The square circuit is tiny
// (~81 KB peak) so 96 KB is plenty.
const HEAP_SIZE: usize = 96 * 1024;
static mut HEAP_MEM: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];

#[global_allocator]
static HEAP: bench_core::TrackingLlff = bench_core::TrackingLlff::empty();

#[link_section = ".start_block"]
#[used]
pub static IMAGE_DEF: hal::block::ImageDef = hal::block::ImageDef::secure_exe();

#[link_section = ".bi_entries"]
#[used]
pub static PICOTOOL_ENTRIES: [hal::binary_info::EntryAddr; 4] = [
    hal::binary_info::rp_cargo_bin_name!(),
    hal::binary_info::rp_cargo_version!(),
    hal::binary_info::rp_program_description!(c"zkmcu: timing oracle (Cortex-M33)"),
    hal::binary_info::rp_program_build_attribute!(),
];

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
    // SAFETY: linker guarantees these symbols are 4-byte aligned and non-overlapping.
    unsafe { core::ptr::copy_nonoverlapping(src, dst, count) }
}

/// Read exactly `buf.len()` bytes from USB serial, blocking until all arrive.
fn read_exact<B: UsbBus>(bench: &mut Bench<'_, B>, buf: &mut [u8]) {
    let mut filled = 0usize;
    while filled < buf.len() {
        bench.dev.poll(&mut [&mut bench.serial]);
        match bench.serial.read(buf.get_mut(filled..).expect("filled <= buf.len() by loop invariant")) {
            Ok(n) if n > 0 => filled += n,
            _ => {}
        }
    }
}

/// Scan incoming bytes until the two-byte knock sequence `\x55\xAA` is found,
/// discarding everything before it.
///
/// The knock sequence does not appear in any firmware TX message (SELFTEST,
/// PROOF, READY, T/F/E response lines) so tty-echo loopback bytes — which
/// arrive before the host writes the knock — are silently consumed here
/// instead of corrupting the proof read that follows.
fn wait_for_knock<B: UsbBus>(bench: &mut Bench<'_, B>) {
    let mut prev = 0u8;
    loop {
        bench.dev.poll(&mut [&mut bench.serial]);
        let mut byte = [0u8; 1];
        if bench.serial.read(&mut byte) == Ok(1) {
            if prev == 0x55 && byte[0] == 0xAA {
                return;
            }
            prev = byte[0];
        }
    }
}

#[hal::entry]
fn main() -> ! {
    // SAFETY: static, called once before any allocation.
    unsafe { HEAP.init(core::ptr::addr_of_mut!(HEAP_MEM) as usize, HEAP_SIZE) }

    init_cycle_counter();
    let pac = hal::pac::Peripherals::take().expect("rp235x PAC once");
    let (timer, usb_bus) = init_rp2350(pac);

    let mut bench = Bench::new(
        &usb_bus,
        timer,
        BenchConfig {
            manufacturer: "zkmcu",
            product: "bench-rp2350-m33-timing-oracle",
            serial: "0001",
        },
    );

    bench.enumerate_for(2_000_000);

    // Parse the square test vector once. VK and public inputs are fixed for
    // all oracle queries; only the proof bytes change.
    let tv = zkmcu_vectors::square().expect("square vector parse");

    let sys_hz = u64::from(SYS_HZ);

    // Boot self-test: verify the embedded proof bytes directly (no USB).
    {
        let selftest = zkmcu_verifier::verify(&tv.vk, &tv.proof, &tv.public);
        let label = match selftest {
            Ok(true)  => "SELFTEST: T (crypto OK)\r\n",
            Ok(false) => "SELFTEST: F (wrong result)\r\n",
            Err(_)    => "SELFTEST: E (verify error)\r\n",
        };
        bench.write_line(label.as_bytes());

        let raw = zkmcu_vectors::SQUARE_PROOF_RAW;
        let mut hex: String<48> = String::new();
        let _ = write!(&mut hex, "PROOF[0..8]:");
        for b in raw.get(..8).unwrap_or_default() {
            let _ = write!(&mut hex, " {b:02x}");
        }
        let _ = write!(&mut hex, "\r\n");
        bench.write_line(hex.as_bytes());
    }

    let mut iter: u32 = 0;

    // Signal readiness. Any tty-echo of these boot messages will be discarded
    // by wait_for_knock in the query loop below.
    bench.write_line(b"READY\r\n");

    loop {
        // Block until the host sends the \x55\xAA knock, consuming any
        // loopback echo bytes that arrived before the knock.
        wait_for_knock(&mut bench);

        let mut proof_bytes = [0u8; 256];
        read_exact(&mut bench, &mut proof_bytes);

        iter = iter.wrapping_add(1);

        let (result, cycles) = measure_cycles(|| {
            let Ok(proof) = zkmcu_verifier::parse_proof(&proof_bytes) else { return b'E' };
            match zkmcu_verifier::verify(&tv.vk, &proof, &tv.public) {
                Ok(true) => b'T',
                Ok(false) => b'F',
                Err(_) => b'E',
            }
        });

        let us = cycles.saturating_mul(1_000_000) / sys_hz;

        let mut out: String<64> = String::new();
        let verdict_char = result as char;
        let _ = write!(&mut out, "{verdict_char} {cycles} {us}\r\n");
        bench.write_line(out.as_bytes());
    }
}
