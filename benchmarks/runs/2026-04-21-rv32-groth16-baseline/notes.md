# 2026-04-21 — Hazard3 RV32 Groth16 baseline

First Groth16/BN254 verify on the Hazard3 RISC-V core of the RP2350, same silicon and clock as the M33 baseline, same verifier source, same test vector. Required one Hazard3-specific fix: clear `mcountinhibit` at boot to ungate `mcycle`. After that, clean numbers on first try.

## Cross-ISA comparison (RP2350 @ 150 MHz, same binary except arch)

| Operation | Cortex-M33 | Hazard3 RV32 | RV32 / M33 | Notes |
|-----------|-----------:|-------------:|-----------:|-------|
| G1 scalar mul (typical) | 16.5 M cyc | **10.8 M cyc** | **0.65×** | RV32 *wins* by ~35% |
| G2 scalar mul (typical) | 31.5 M cyc | 42.5 M cyc | 1.35× | RV32 loses by ~35% |
| BN254 pairing | 80.0 M cyc | 105.1 M cyc | 1.31× | RV32 loses by ~31% |
| **Groth16 verify** | **148.0 M cyc / 988 ms** | **198.3 M cyc / 1322 ms** | **1.34×** | RV32 loses by ~34% |

Correctness: `ok=true` on every iteration of every benchmark on both cores.

Determinism: Groth16 verify variance across iterations on Hazard3 is 57,770 cycles on a 198 M median → **0.029%**. Matches M33 to within noise. Both cores, in-order + fixed-clock + single-tenant, beat any desktop substrate-bn run by multiple orders of magnitude on predictability.

## The surprising result: G1 wins on RV32

The typical story for RISC-V vs. Cortex-M on big-integer arithmetic is that ARM's DSP multiply (SMLAL / UMAAL) beats RV32I+M at 32×32→64 multiplies. Hazard3 has standard RV32 `mul` / `mulh` / `mulhu` (plus Zba / Zbb bit-manipulation) but no equivalent of UMAAL. Expected outcome: Hazard3 slower on every big-int op.

Actual result: Hazard3 is **35% faster at G1 scalar multiplication**. Same `substrate-bn` source, same optimizer, same clock.

Why (speculative, worth verifying with a disassembly dive later):

- **More registers.** RV32 has 31 general-purpose registers (x1–x31) vs. Thumb-2's 13 effective (r0–r12) plus a spill-prone r14/r15 pair. Big-integer schoolbook multiplication produces a lot of intermediate values; fewer register spills on RV32 means less load/store traffic.
- **Zbb leading-zero count.** Montgomery reduction's choice of quotient digit benefits from `CLZ` instructions. Hazard3's Zbb has `clz` / `ctz`; Thumb-2's CLZ is there too but the enclosing sequence differs.
- **Code density tradeoffs.** Thumb-2 is denser (16/32-bit mixed), which helps icache. RV32IMAC has compressed 16-bit instructions as well. Maybe a wash.
- **LLVM backend differences.** The substrate-bn arithmetic kernels get lowered differently per target. The RV32 backend might have gotten more recent attention for generic integer optimization. Checking the emitted assembly for a Fq multiplication on each target would nail this.

## The opposite result on G2 / pairing

G2 is Fq2 (tower field over Fq), pairing is Fq12. More memory accesses, more allocations, more temporary objects. RV32 loses on these:

- Possible cause: RV32 heap/Vec allocation hot path is heavier than on M33 (linked-list allocator traversal — same Rust code on both, but maybe different generated code).
- Possible cause: Fq2 multiplication uses 3× Fq mul per op (Karatsuba) — amplifying any per-Fq-op overhead the compiler introduced for Fq2.

Worth a disassembly / profile comparison before publishing the blog post, but the raw-number story is already clean and publishable.

## What this means for the project

1. **Fully portable verifier confirmed.** One Rust source tree produces working Groth16 verify binaries for both ARM and RISC-V silicon. No per-ISA verifier code.
2. **Cortex-M33 is the M33+RV32 winner by ~34% on the full verify.** For single-purpose verifier hardware, M33 is the right default target today.
3. **RV32 is competitive, not embarrassing.** 1.3 s Groth16 verify on a $7 MCU's RISC-V core is still plenty for credential checks, IoT attestation, offline ticket validation.
4. **First public cross-ISA pairing number on identical silicon.** The prior-art survey flagged this as a gap; it's filled.

## Things unmeasured but worth measuring soon

- Peak stack usage (paint `.stack` on boot, grep for the high-water mark).
- `opt-level = 3` vs. the current `opt-level = "s"` on both cores — size-optimised code may give up some speed on big-int math.
- Effect of enabling the Hazard3 B-extension bit-manip helpers via target-feature flags (we currently use plain `riscv32imac`; the core exposes Zbb/Zba/Zbc/Zbs).
- Disassembly diff between M33 and RV32 for one `substrate-bn` Fq multiplication to explain the G1 surprise.

## Reproduction

```bash
cd crates/bench-rp2350-rv32
cargo build --release
scp target/riscv32imac-unknown-none-elf/release/bench-rp2350-rv32 pi:/tmp/bench-rv32.elf
# Pico in BOOTSEL:
picotool load -v -x -t elf /tmp/bench-rv32.elf
cat /dev/ttyACM0
```

Firmware has to call `csrw mcountinhibit, zero` once before any `mcycle` read — otherwise every cycle count is zero. See the referenced finding.
