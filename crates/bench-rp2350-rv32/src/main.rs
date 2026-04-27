#![no_std]
#![no_main]
// Same justifications as bench-rp2350-m33: integer math, long init, init-panic safety.
#![allow(clippy::integer_division)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::panic)]

use core::fmt::Write as _;
use core::mem::MaybeUninit;

use bench_core::{
    cycles_u64, init_cycle_counter, init_rp2350, measure_cycles, measure_stack_peak, Bench,
    BenchConfig, TrackingLlff, UsbBus, SYS_HZ,
};
use bn::{miller_loop_batch, pairing, Fr, Group, Gt, G1, G2};
use heapless::String;
use panic_halt as _;
use rp235x_hal as hal;
use substrate_bn as bn;

#[global_allocator]
static HEAP: TrackingLlff = TrackingLlff::empty();

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
            product: "bench-rp2350-rv32",
            serial: "0001",
        },
    );

    bench.enumerate_for(2_000_000);
    bench.write_line(b"zkmcu boot: heap=256K sys=150MHz core=hazard3\r\n");

    let test_vector = zkmcu_vectors::square().expect("square test vector parse");
    let squares_5 = zkmcu_vectors::squares_5().expect("squares-5 test vector parse");
    let semaphore = zkmcu_vectors::semaphore_depth_10().expect("semaphore depth-10 parse");
    let poseidon_3 = zkmcu_vectors::poseidon_depth_3().expect("poseidon depth-3 parse");
    let poseidon_10 = zkmcu_vectors::poseidon_depth_10().expect("poseidon depth-10 parse");

    let sys_hz: u64 = u64::from(SYS_HZ);

    // Boot-time stack + cycle + heap measurements. The heap fields are new on
    // RV32: prior baselines ran without TrackingHeap and reported heap_peak=0.
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

        let mut seed = [0u8; 32];
        let seed_src = u64::from(iter).wrapping_add(cycles_u64());
        seed[..8].copy_from_slice(&seed_src.to_le_bytes());
        seed[31] = 0;
        let s = Fr::from_slice(&seed).unwrap_or_else(|_| Fr::one());

        bench.print_marker(iter, b"g1mul start\r\n");
        let (p, c_g1) = measure_cycles(|| G1::one() * s);
        core::hint::black_box(&p);
        bench.print_result(iter, "g1mul", c_g1, sys_hz);

        bench.print_marker(iter, b"g2mul start\r\n");
        let (q, c_g2) = measure_cycles(|| G2::one() * s);
        core::hint::black_box(&q);
        bench.print_result(iter, "g2mul", c_g2, sys_hz);

        bench.print_marker(iter, b"pairing start\r\n");
        let (gt, c_pair) = measure_cycles(|| pairing(p, q));
        core::hint::black_box(&gt);
        bench.print_result(iter, "pairing", c_pair, sys_hz);

        loop_verify(
            &mut bench,
            iter,
            "groth16_verify",
            &test_vector.vk,
            &test_vector.proof,
            &test_vector.public,
            sys_hz,
        );
        loop_verify(
            &mut bench,
            iter,
            "groth16_verify_semaphore",
            &semaphore.vk,
            &semaphore.proof,
            &semaphore.public,
            sys_hz,
        );
        loop_verify(
            &mut bench,
            iter,
            "groth16_verify_poseidon_d3",
            &poseidon_3.vk,
            &poseidon_3.proof,
            &poseidon_3.public,
            sys_hz,
        );
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
        let ic0 = test_vector.vk.ic.first().expect("ic[0]");
        let ic1 = test_vector.vk.ic.get(1).expect("ic[1]");
        let pub0 = test_vector.public.first().expect("public[0]");
        let (vk_x_sq, c_vk_x_tiny) = measure_cycles(|| *ic0 + (*ic1 * *pub0));
        core::hint::black_box(&vk_x_sq);
        bench.print_result(iter, "vk_x_tiny_scalar", c_vk_x_tiny, sys_hz);

        let pq_ic0 = poseidon_3.vk.ic.first().expect("pq ic[0]");
        let pq_ic1 = poseidon_3.vk.ic.get(1).expect("pq ic[1]");
        let pq_pub0 = poseidon_3.public.first().expect("pq public[0]");
        let (_, c_vk_x_full) = measure_cycles(|| *pq_ic0 + (*pq_ic1 * *pq_pub0));
        bench.print_result(iter, "vk_x_full_scalar", c_vk_x_full, sys_hz);

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
