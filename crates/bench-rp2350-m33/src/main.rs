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

use bench_core::{
    cycles_u64, init_cycle_counter, init_rp2350, measure_cycles, measure_stack_peak, Bench,
    BenchConfig, TrackingLlff, UsbBus, SYS_HZ,
};
use bn::{miller_loop_batch, pairing, Fq, Fr, Group, Gt, G1, G2};
use heapless::String;
use panic_halt as _;
use rp235x_hal as hal;
use substrate_bn as bn;

#[global_allocator]
static HEAP: TrackingLlff = TrackingLlff::empty();

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

    init_cycle_counter();
    let pac = hal::pac::Peripherals::take().expect("rp235x PAC once");
    let (timer, usb_bus) = init_rp2350(pac);

    let mut bench = Bench::new(
        &usb_bus,
        timer,
        BenchConfig {
            manufacturer: "zkmcu",
            product: "bench-rp2350-m33",
            serial: "0001",
        },
    );

    bench.enumerate_for(2_000_000);
    bench.write_line(b"zkmcu boot: heap=96K sys=150MHz core=cortex-m33\r\n");

    // Self-test the UMAAL asm before any measurement. If the asm miscomputes
    // `Fq::mul`, any verify number we emit below would be meaningless, so
    // halt instead of publishing a corrupted baseline.
    if !run_umaal_kat(&mut bench) {
        loop {
            bench.dev.poll(&mut [&mut bench.serial]);
        }
    }

    // Parse the test vector once at startup so we don't time the parse or thrash the heap.
    let test_vector = zkmcu_vectors::square().expect("square test vector parse");
    let squares_5 = zkmcu_vectors::squares_5().expect("squares-5 test vector parse");
    let semaphore = zkmcu_vectors::semaphore_depth_10().expect("semaphore depth-10 parse");
    let poseidon_3 = zkmcu_vectors::poseidon_depth_3().expect("poseidon depth-3 parse");
    let poseidon_10 = zkmcu_vectors::poseidon_depth_10().expect("poseidon depth-10 parse");

    let sys_hz: u64 = u64::from(SYS_HZ);

    // One-shot stack + cycle + heap measurement at boot, for every test vector.
    // Each row: peak stack, peak heap, verify latency for one circuit size.
    // The pairs across rows give the scaling of verify cost with public
    // inputs without needing to tear down the main loop.
    boot_measure(
        &mut bench,
        sys_hz,
        test_vector.name,
        test_vector.vk.ic.len(),
        test_vector.public.len(),
        &test_vector.vk,
        &test_vector.proof,
        &test_vector.public,
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
    boot_measure(
        &mut bench,
        sys_hz,
        semaphore.name,
        semaphore.vk.ic.len(),
        semaphore.public.len(),
        &semaphore.vk,
        &semaphore.proof,
        &semaphore.public,
    );
    boot_measure(
        &mut bench,
        sys_hz,
        poseidon_3.name,
        poseidon_3.vk.ic.len(),
        poseidon_3.public.len(),
        &poseidon_3.vk,
        &poseidon_3.proof,
        &poseidon_3.public,
    );
    boot_measure(
        &mut bench,
        sys_hz,
        poseidon_10.name,
        poseidon_10.vk.ic.len(),
        poseidon_10.public.len(),
        &poseidon_10.vk,
        &poseidon_10.proof,
        &poseidon_10.public,
    );

    let mut iter: u32 = 0;

    loop {
        iter = iter.wrapping_add(1);

        // ---- seed scalar (non-constant so the compiler can't fold anything) ----
        let mut seed = [0u8; 32];
        let seed_src = u64::from(iter).wrapping_add(cycles_u64());
        seed[..8].copy_from_slice(&seed_src.to_le_bytes());
        // Clear the top byte so the value is < field modulus.
        seed[31] = 0;
        let s = Fr::from_slice(&seed).unwrap_or_else(|_| Fr::one());

        // ---- G1 scalar mul ----
        bench.print_marker(iter, b"g1mul start\r\n");
        let (p, c_g1) = measure_cycles(|| G1::one() * s);
        core::hint::black_box(&p);
        bench.print_result(iter, "g1mul", c_g1, sys_hz);

        // ---- G2 scalar mul ----
        bench.print_marker(iter, b"g2mul start\r\n");
        let (q, c_g2) = measure_cycles(|| G2::one() * s);
        core::hint::black_box(&q);
        bench.print_result(iter, "g2mul", c_g2, sys_hz);

        // ---- Pairing ----
        bench.print_marker(iter, b"pairing start\r\n");
        let (gt, c_pair) = measure_cycles(|| pairing(p, q));
        core::hint::black_box(&gt);
        bench.print_result(iter, "pairing", c_pair, sys_hz);

        // ---- Full Groth16 verify (the headline number) ----
        loop_verify(
            &mut bench,
            iter,
            "groth16_verify",
            &test_vector.vk,
            &test_vector.proof,
            &test_vector.public,
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

        // ---- Real-world data point: Semaphore depth-10 verify ----
        loop_verify(
            &mut bench,
            iter,
            "groth16_verify_semaphore",
            &semaphore.vk,
            &semaphore.proof,
            &semaphore.public,
            sys_hz,
        );

        // ---- Poseidon Merkle membership: depth 3 (8 leaves, 739 constraints) ----
        loop_verify(
            &mut bench,
            iter,
            "groth16_verify_poseidon_d3",
            &poseidon_3.vk,
            &poseidon_3.proof,
            &poseidon_3.public,
            sys_hz,
        );

        // ---- Poseidon Merkle membership: depth 10 (1024 leaves, 2461 constraints) ----
        loop_verify(
            &mut bench,
            iter,
            "groth16_verify_poseidon_d10",
            &poseidon_10.vk,
            &poseidon_10.proof,
            &poseidon_10.public,
            sys_hz,
        );

        // ---- Verify cost breakdown: vk_x / multi-Miller loop / final exp --------
        //
        // vk_x with a tiny scalar (y=9, square circuit) — establishes the floor.
        let ic0 = test_vector.vk.ic.first().expect("ic[0]");
        let ic1 = test_vector.vk.ic.get(1).expect("ic[1]");
        let pub0 = test_vector.public.first().expect("public[0]");
        let (vk_x_sq, c_vk_x_tiny) = measure_cycles(|| *ic0 + (*ic1 * *pub0));
        core::hint::black_box(&vk_x_sq);
        bench.print_result(iter, "vk_x_tiny_scalar", c_vk_x_tiny, sys_hz);

        // vk_x with a full 254-bit scalar (poseidon root) — typical cost.
        let pq_ic0 = poseidon_3.vk.ic.first().expect("pq ic[0]");
        let pq_ic1 = poseidon_3.vk.ic.get(1).expect("pq ic[1]");
        let pq_pub0 = poseidon_3.public.first().expect("pq public[0]");
        let (_, c_vk_x_full) = measure_cycles(|| *pq_ic0 + (*pq_ic1 * *pq_pub0));
        bench.print_result(iter, "vk_x_full_scalar", c_vk_x_full, sys_hz);

        // Multi-Miller loop over 4 pairs — the shared accumulation step in verify.
        // Note: miller_loop_batch takes (G2, G1) pairs (reversed from pairing_batch).
        let ml_pairs = [
            (test_vector.proof.b, -test_vector.proof.a),
            (test_vector.vk.beta,  test_vector.vk.alpha),
            (test_vector.vk.gamma, vk_x_sq),
            (test_vector.vk.delta, test_vector.proof.c),
        ];
        let (ml, c_miller) = measure_cycles(|| {
            miller_loop_batch(&ml_pairs).unwrap_or_else(|_| Gt::one())
        });
        core::hint::black_box(&ml);
        bench.print_result(iter, "miller_loop_4pair", c_miller, sys_hz);

        // Final exponentiation — applied once to the accumulated Miller loop product.
        let (_, c_final_exp) = measure_cycles(|| ml.final_exponentiation());
        bench.print_result(iter, "final_exp", c_final_exp, sys_hz);

        bench.pace(1_000_000);
    }
}

fn loop_verify<B: UsbBus>(
    bench: &mut Bench<'_, B>,
    iter: u32,
    label: &str,
    vk: &zkmcu_verifier::VerifyingKey,
    proof: &zkmcu_verifier::Proof,
    public: &[zkmcu_verifier::Fr],
    sys_hz: u64,
) {
    let mut marker: String<64> = String::new();
    let _ = write!(&mut marker, "[{iter}] {label} start\r\n");
    bench.write_line(marker.as_bytes());

    let (result, cycles) = measure_cycles(|| zkmcu_verifier::verify(vk, proof, public));
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
    vk: &zkmcu_verifier::VerifyingKey,
    proof: &zkmcu_verifier::Proof,
    public: &[zkmcu_verifier::Fr],
) {
    // Reset peak-heap tracking right before the measured call, so the reported
    // figure is the peak during this one verify and not cumulative from earlier
    // parses / setup.
    HEAP.reset_peak();
    let heap_before = HEAP.current();

    let (verify_ok, stack_peak, cycles) =
        measure_stack_peak(|| zkmcu_verifier::verify(vk, proof, public));

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
fn run_umaal_kat<B: UsbBus>(bench: &mut Bench<'_, B>) -> bool {
    let bytes = zkmcu_vectors::UMAAL_KAT;
    let rec = zkmcu_vectors::UMAAL_KAT_RECORD_SIZE;
    let n = bytes.len() / rec;

    let (outcome, cycles) = measure_cycles(|| -> Result<(), usize> {
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
                return Err(i);
            }
        }
        Ok(())
    });

    if let Err(i) = outcome {
        let mut out: String<128> = String::new();
        let _ = writeln!(
            &mut out,
            "UMAAL KAT: FAIL at record {i} of {n}, asm diverges from Rust reference"
        );
        bench.write_line(out.as_bytes());
        return false;
    }

    let us = cycles.saturating_mul(1_000_000) / u64::from(SYS_HZ);
    let mut out: String<128> = String::new();
    let _ = writeln!(
        &mut out,
        "UMAAL KAT: {n}/{n} OK ({cycles} cycles, {us} us, asm agrees with Rust reference)"
    );
    bench.write_line(out.as_bytes());
    true
}
