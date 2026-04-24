#![no_std]
#![no_main]
#![allow(clippy::integer_division)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::panic)]

extern crate alloc;

use core::fmt::Write as _;
use core::mem::MaybeUninit;

use bench_core::{
    cycles_u64, init_cycle_counter, init_rp2350, measure_cycles, measure_stack_peak, Bench,
    BenchConfig, TrackingLlff, UsbBus, SYS_HZ,
};
use bls12_381::{pairing, G1Affine, G2Affine, Scalar};
use heapless::String;
use panic_halt as _;
use rp235x_hal as hal;
use zkmcu_verifier_bls12 as zkvb;

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

#[global_allocator]
static HEAP: TrackingLlff = TrackingLlff::empty();

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

/// Parsed test vector: VK + proof + public inputs, pre-decoded at boot so the
/// verify loop doesn't pay parse cost or thrash the allocator.
struct TestVector {
    name: &'static str,
    vk: zkvb::VerifyingKey,
    proof: zkvb::Proof,
    public: alloc::vec::Vec<zkvb::Fr>,
}

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

    init_cycle_counter();
    let pac = hal::pac::Peripherals::take().expect("rp235x PAC once");
    let (timer, usb_bus) = init_rp2350(pac);

    let mut bench = Bench::new(
        &usb_bus,
        timer,
        BenchConfig {
            manufacturer: "zkmcu",
            product: "bench-rp2350-m33-bls12",
            serial: "0001",
        },
    );

    bench.enumerate_for(2_000_000);
    bench.write_line(b"zkmcu boot: heap=256K sys=150MHz core=cortex-m33 curve=bls12-381\r\n");

    let square = parse_vector("square", SQUARE_VK, SQUARE_PROOF, SQUARE_PUBLIC);
    let squares_5 = parse_vector("squares-5", SQUARES_5_VK, SQUARES_5_PROOF, SQUARES_5_PUBLIC);

    let sys_hz: u64 = u64::from(SYS_HZ);

    boot_measure(
        &mut bench,
        sys_hz,
        square.name,
        square.vk.ic.len(),
        square.public.len(),
        &square.vk,
        &square.proof,
        &square.public,
    );
    boot_measure(
        &mut bench,
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
        let seed_src = u64::from(iter).wrapping_add(cycles_u64());
        seed[..8].copy_from_slice(&seed_src.to_le_bytes());
        seed[31] = 0;
        let s: Scalar = Option::from(Scalar::from_bytes(&seed))
            .expect("seed < r by construction (top byte cleared)");

        // ---- G1 scalar mul ----
        bench.print_marker(iter, b"g1mul start\r\n");
        let (p, c_g1) = measure_cycles(|| G1Affine::generator() * s);
        core::hint::black_box(&p);
        bench.print_result(iter, "g1mul", c_g1, sys_hz);

        // ---- G2 scalar mul ----
        bench.print_marker(iter, b"g2mul start\r\n");
        let (q, c_g2) = measure_cycles(|| G2Affine::generator() * s);
        core::hint::black_box(&q);
        bench.print_result(iter, "g2mul", c_g2, sys_hz);

        // ---- Pairing. Affine conversion is done outside the timed window
        // to match the 2026-04-22-m33-bls12-baseline methodology.
        bench.print_marker(iter, b"pairing start\r\n");
        let p_aff = G1Affine::from(p);
        let q_aff = G2Affine::from(q);
        let (gt, c_pair) = measure_cycles(|| pairing(&p_aff, &q_aff));
        core::hint::black_box(&gt);
        bench.print_result(iter, "pairing", c_pair, sys_hz);

        // ---- Full Groth16 verify, the headline number ----
        loop_verify(
            &mut bench,
            iter,
            "groth16_verify",
            &square.vk,
            &square.proof,
            &square.public,
            sys_hz,
        );

        // ---- Scaling data point: 5-public-input verify ----
        loop_verify(
            &mut bench,
            iter,
            "groth16_verify_sq5",
            &squares_5.vk,
            &squares_5.proof,
            &squares_5.public,
            sys_hz,
        );

        bench.pace(1_000_000);
    }
}

fn loop_verify<B: UsbBus>(
    bench: &mut Bench<'_, B>,
    iter: u32,
    label: &str,
    vk: &zkvb::VerifyingKey,
    proof: &zkvb::Proof,
    public: &[zkvb::Fr],
    sys_hz: u64,
) {
    let mut marker: String<64> = String::new();
    let _ = write!(&mut marker, "[{iter}] {label} start\r\n");
    bench.write_line(marker.as_bytes());

    let (result, cycles) = measure_cycles(|| zkvb::verify(vk, proof, public));
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
    bench.write_line(out.as_bytes());
}

#[allow(clippy::too_many_arguments)]
fn boot_measure<B: UsbBus>(
    bench: &mut Bench<'_, B>,
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

    let (verify_ok, stack_peak, cycles) = measure_stack_peak(|| zkvb::verify(vk, proof, public));

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
    bench.write_line(out.as_bytes());
}
