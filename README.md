# zkmcu

`no_std` Rust family of SNARK and STARK verifiers — and now a STARK prover — for microcontrollers. Three proof systems, one repo shape: Groth16 on BN254 (EIP-197 wire format, on `substrate-bn`), Groth16 on BLS12-381 (EIP-2537 wire format, on zkcrypto `bls12_381`), and winterfell STARK on BabyBear+Quartic (winterfell 0.13). Same parser shape, same firmware template, different proof system underneath.

The prover is the new part. 48 ms prove + 29 ms verify on a Cortex-M33 @ 150 MHz, 64 KB heap peak, no OS. As far as I can find nobody has published a STARK prover running on bare-metal embedded hardware before — closest prior art (FibRace) needs 3 GB of RAM, we're doing it in 64 KB on a $7 chip.

First target is the Raspberry Pi Pico 2 W. RP2350 is a fun chip because it has **both** an ARM Cortex-M33 and a RISC-V Hazard3 core on the same die at the same clock, wich makes the cross-ISA comparison drama-free. One Rust source tree, two ISAs, same binary build pipeline, six firmware crates.

Docs site: [zkmcu.dev](https://zkmcu.dev).

## Headline

Pico 2 W at 150 MHz, measured on-device with `DWT::cycle_count` on M33 and `mcycle` on RV32. Every iteration `ok=true`. No hand-tuning, stock upstream crypto crates.

| prove + verify | Cortex-M33 | Hazard3 RV32 | heap peak | proof size |
|---|---:|---:|---:|---:|
| **STARK threshold-check N=64** (BabyBear+Quartic, 21-bit) | **48 ms + 29 ms** | not yet | **64 KB** | 5.8 KB |
| **STARK Fibonacci N=256** (BabyBear+Quartic, 21-bit) | **148 ms + 33 ms** | 215 ms + 42 ms | 248 KB | 8.9 KB |

| verify only | Cortex-M33 | Hazard3 RV32 | RV32 / M33 | proof size |
|---|---:|---:|---:|---:|
| STARK Fibonacci-1024 (Goldilocks, 95-bit) | **75 ms** | 112 ms | 1.51× | 30.9 KB |
| **Groth16 / BN254, real Semaphore v4** (4 pub inputs) | **1,176 ms** | 1,564 ms | 1.33× | 256 B |
| Groth16 / BN254, 1 public input | 962 ms | 1,341 ms | 1.39× | 256 B |
| Groth16 / BLS12-381, 1 public input | 2,015 ms | 5,151 ms | 2.56× | 512 B |

STARK verify is **15-27x faster than Groth16** on the same silicon. Classic throughput-for-bandwidth swap: Groth16 is 256 B but takes ~1-2 seconds, STARK is 30 KB but takes 75 ms. Pick based on whether the transport is bandwidth-bound (LoRa, NFC, pick Groth16) or verify-latency-bound (hot loop, pick STARK).

All three verifier families fit the **128 KB SRAM tier** during verify (~97-100 KB total RAM on M33), wich is the tier of most hardware-wallet-grade silicon: `nRF52832`, `STM32F405`, Ledger ST33, Infineon SLE78. As far as I can tell this is the first public `no_std` Rust family that covers all three proof systems under 128 KB at production-grade security. Full gap analysis in `research/prior-art/main.typ`.

## STARK prover on bare metal

Yeah so this is the part I'm most excited about right now.

48 ms prove, 29 ms verify, 64 KB heap peak on a Cortex-M33 @ 150 MHz. No OS, 512 KB total SRAM, `heap_after = 10 bytes` every iteration. Consistent over 2000+ iterations on device.

And its not Fibonacci — the threshold-check circuit proves `value < threshold` for a sensor reading. Bit-decomposes `diff = threshold - value - 1` over 32 rows, boundary assertion at row 32 certifies no underflow, therefore `value < threshold`. An embedded device can attest its sensor reading without the verifier trusting the firmware. Unforgeable.

FibRace needs 3 GB of RAM to run the prover. We're doing it in 64 KB. Thats a 50,000x memory reduction on a $7 chip.

| | Cortex-M33 |
|-|---:|
| prove (threshold-check N=64) | **48 ms** |
| verify | **29 ms** |
| heap peak | **64 KB** |
| proof size | 5.8 KB |
| security (conjectured) | 21 bit |
| heap after verify | 10 bytes |

BabyBear+Quartic, 11 FRI queries. Full page: [zkmcu.dev/stark-prover](https://zkmcu.dev/stark-prover/).

## STARK verify in 75 ms

Honestly the thing I'm most happy with. `zkmcu-verifier-stark` is a `no_std` wrapper around winterfell 0.13 that exposes a zkmcu-shaped verify API for Goldilocks-field STARKs. First on-silicon measurement 2026-04-23, full production-grade-security config locked in 2026-04-24:

- Fibonacci AIR at `N = 1024`
- `FieldExtension::Quadratic` over Goldilocks → **95-bit conjectured STARK security** (matches winterfell's own reference config)
- `MinConjecturedSecurity(95)` enforced by the verifier, so a weaker proof is rejected even if the crypto checks out
- Blake3-256 hash, binary Merkle tree vector commitment
- `embedded-alloc::TlsfHeap` as the global allocator (see the determinism section below for why this matters)

| | Cortex-M33 | Hazard3 RV32 |
|-|---:|---:|
| Median verify | **75 ms** | 112 ms |
| Std-dev variance | **0.081 %** | 0.110 % |
| Peak heap | 93.5 KB | ~93 KB |
| Peak stack | 5.6 KB | 5.5 KB |
| **Total RAM** | **~100 KB** | ~100 KB |

The stack peak is what surprised me most. Groth16 paths run 15-20 KB of stack, STARK runs 5.6 KB. Winterfell just routes everything through the heap allocator instead of stack frames, and the cost of that design choice is actually *not* a bigger stack. More on the heap side below.

## Deterministic timing is the underrated feature

Ok so here's the thing that took me 4 iterations to figure out and it's the best methodology finding in the project IMO. The STARK verify path makes **~400 `Vec` allocations internally** for FRI state, auth-path parsing, composition poly scratch. With the stock `LlffHeap` (linked-list first-fit) allocator, each iteration mutates the free list slightly differently than the last one, and that free-list evolution shows up as iteration-to-iteration timing variance around 0.25-0.46 %.

For reference, the Groth16 verifiers on the same silicon measure ~0.03-0.07 % variance. They barely allocate during verify. So STARK was 5-10x noisier than what the silicon can actually resolve, wich is bad for any side-channel-adjacent use case.

Tried three allocators, measured all three, ended up with a matrix:

| Allocator | M33 median | M33 std-dev | M33 heap peak | 128 KB tier? |
|---|---:|---:|---:|:---:|
| LlffHeap (linked-list first-fit) | 69.7 ms | ~0.13 % IQR | 93.5 KB | ✓ |
| **TlsfHeap (O(1) two-level segregated fit)** | **74.7 ms** | **0.081 %** | **93.5 KB** | **✓** |
| BumpAlloc (watermark-reset, benchmark only) | 67.9 ms | 0.080 % | 314 KB | ✗ |

TlsfHeap is the production pick. You pay ~5 ms of median verify time over LlffHeap (~20 ms on RV32, Hazard3 pays more for the bitmap walks) and get silicon-baseline 0.08 % variance while staying on the 128 KB tier. For hardware-wallet firmware where verify runs at human action speed, that's indistinguishable.

The bump allocator (`zkmcu-bump-alloc`) is a custom ~200-line `no_std` `GlobalAlloc` I wrote for the variance experiment: atomic CAS bump pointer, no-op dealloc, in-place realloc when the resized allocation is on top of the bump, watermark save/restore. Use it as a measurement tool (confirms the crypto itself is deterministic, variance isn't the crypto's fault) or as a standalone primitive for any benchmark loop that wants byte-identical allocator state per iteration. Not a production allocator, memory peak is 3x LlffHeap's.

Full story: [zkmcu.dev/determinism](https://zkmcu.dev/determinism/) and `research/reports/2026-04-24-stark-allocator-matrix.typ`.

### The cross-ISA twist

Bonus finding that surprised me: **the allocator choice can swing the Cortex-M33-vs-Hazard3 ratio by 30 %**.

| STARK config | RV32 / M33 |
|---|---:|
| BumpAlloc (allocator overhead stripped) | **1.21×** ← pure crypto ratio |
| LlffHeap | 1.33× |
| TlsfHeap | 1.51× |

BumpAlloc is branch-free on the happy path, so it gives the honest "pure Blake3 + Goldilocks Fp2 arithmetic" cross-ISA ratio: 1.21x. LlffHeap's free-list walk costs Hazard3 more per op (weaker branch prediction on pointer chases), TlsfHeap's bitmap walks cost Hazard3 even more (more small conditional branches). An "M33 vs Hazard3" STARK benchmark using a stock allocator is partially measuring the allocator, not the workload. Ofcourse that goes in the limitations section of every cross-ISA report in this project going forward.

## Real-world circuit: Semaphore

Synthetic `x^2 = y` circuits are fine for bench infrastructure but nobody actually uses them. So I took a real [Semaphore](https://semaphore.pse.dev/) v4.14.2 Groth16 proof (Merkle tree depth 10, 4 public inputs: merkle root, nullifier, hashed message, hashed scope), generated by snarkjs under the production trusted setup, and verified it through `zkmcu-verifier` on the same hardware:

| circuit | Cortex-M33 | Hazard3 RV32 | RV32 / M33 |
|---|---:|---:|---:|
| **Semaphore v4.14.2 depth-10 verify** | **1,176 ms** | 1,564 ms | 1.33× |

That's the *same* VK + proof bytes the Ethereum Semaphore verifier precompile accepts, running unmodified on a $7 MCU. Variance 0.030 % on both cores, tightest of any measurement in the project. Prediction written down ahead of time was 1,160 ms M33 / 1,620 ms RV32, measured 1,176 / 1,564 (Δ +1.4 % / −3.5 %). Full setup: `research/reports/2026-04-22-semaphore-baseline.typ` and [zkmcu.dev/semaphore](https://zkmcu.dev/semaphore/).

Heads up for anyone designing embedded-ZK circuits: on BN254 Cortex-M33, each extra *big-scalar* public input (merkle root, nullifier, hash output) costs **~71 ms** on top of the 962 ms single-pub-input baseline. Small-integer public inputs cost ~3 ms, so a 24x gap caused by `substrate-bn`'s sliding-window NAF short-circuiting on low-Hamming-weight scalars. Plan for the big-scalar regime, it's what real circuits hit.

## Per-op breakdown, BN254 (EIP-197)

| op | Cortex-M33 | Hazard3 RV32 | RV32 / M33 |
|---|---:|---:|---:|
| G1 scalar mul (typical) | **62 ms** | 65 ms | 1.05× |
| G2 scalar mul (typical) | **207 ms** | 283 ms | 1.37× |
| pairing | **535 ms** | 707 ms | 1.32× |
| **Groth16 verify** (1 pub input) | **962 ms** | 1,341 ms | 1.39× |

## Per-op breakdown, BLS12-381 (EIP-2537)

| op | Cortex-M33 | Hazard3 RV32 | RV32 / M33 |
|---|---:|---:|---:|
| G1 scalar mul | **847 ms** | 1,427 ms | 1.69× |
| G2 scalar mul | **523 ms** | 1,003 ms | 1.92× |
| pairing | **607 ms** | 1,975 ms | 3.26× |
| **Groth16 verify** (1 pub input) | **2,015 ms** | 5,151 ms | 2.56× |

These are baselines, not optimized numbers. The M33 has DSP intrinsics (`SMLAL`, `UMAAL`) for Montgomery reduction that neither `substrate-bn` nor `bls12_381` touch out of the box, and Hazard3 has the `Zbb`/`Zba`/`Zbc` bit-manip extensions sitting unused. BN254 on M33 is the first one I actually did, next section.

## UMAAL asm on Cortex-M33 (BN254)

Hand-written ARMv8-M assembly for `substrate-bn`'s `mul_reduce` (the Montgomery multiplication primitive) using UMAAL on 32-bit limbs, placed in SRAM at boot so the hottest function sidesteps the RP2350's 16 KB XIP instruction cache entirely. Lives in a fork of `substrate-bn` at `vendor/bn` behind a `cortex-m33-asm` Cargo feature; host builds and all 34 upstream tests still use the Rust path unchanged.

| op | BN254, stock | BN254, UMAAL asm | Δ |
|---|---:|---:|---:|
| **Groth16 verify** (1 pub input) | 962 ms | **641 ms** | -33 % |
| **Groth16 verify** (Semaphore depth-10) | 1,176 ms | **761 ms** | -35 % |
| BN254 pairing | 535 ms | **350 ms** | -35 % |
| G1 scalar mul (typical) | 62 ms | **33 ms** | -47 % |
| G2 scalar mul (typical) | 207 ms | **150 ms** | -28 % |

The lever is that LLVM's M-profile instruction selector emits zero UMAAL instructions for `substrate-bn`'s `u128`-based schoolbook even with `target-cpu=cortex-m33`. The asm does it by hand: 128 UMAAL inner steps per `mul_reduce`, operand rows kept register-resident in r4–r11 across each phase, `.text` footprint of the replacement primitive at 50 % of LLVM's original output. Moving it to SRAM was worth another 1-2 % on its own and keeps 2 KB of XIP-cache pressure off everything else in the pairing code.

RV32 and BLS12-381 are still at their baselines. UMAAL is ARM-specific; Hazard3's `Zbc` carry-less-mul extension is the analogous lever for RISC-V and hasn't been tried yet. Full arc from the 988 ms crates.io baseline through the four layered optimizations: `research/reports/2026-04-23-umaal-sram-groth16.typ`.

## Memory

Directly measured on-device via stack painting + a tracking heap wrapper. All three verifier families on Cortex-M33:

| | BN254 Groth16 | BLS12 Groth16 | STARK (TlsfHeap) |
|-|---:|---:|---:|
| peak stack during verify | 15.6 KB | 19.4 KB | **5.6 KB** |
| peak heap during verify  | 81.3 KB | 79.4 KB | 93.5 KB |
| heap arena configured     | 96 KB | 256 KB | 256 KB |
| **total RAM during verify** | **~97 KB** | **~99 KB** | **~100 KB** |

All three fit the 128 KB hardware-wallet tier. BLS12-381 on zkcrypto actually uses *less* heap than BN254 on substrate-bn (79 KB vs 81 KB) because zkcrypto keeps Miller-loop line coefficients in stack-allocated `G2Prepared` where substrate-bn heap-allocates an Fq12 polynomial workspace. STARK's 5.6 KB stack is ~3x smaller than either Groth16 path because winterfell routes its verify state through the heap, not the stack.

## Why

The thing that annoyed me into building this is that every existing "ZK on embedded" project I could find either runs under Linux on something like a Pi Zero, or it's a paper with no code. `ZPiE` (2021) is the closest published thing and it needs a full OS. For actual hardware-wallet-class devices (128 KB SRAM, no MMU, no Linux), there was nothing. So yeah I wrote it.

BLS12-381 got added to prove the approach generalizes across curves and ecosystems (BN254 for Ethereum-era circuits, BLS12-381 for Zcash / sync-committee / Filecoin). STARK got added because Groth16 verify is slow (1-2 seconds) and I wanted to see if a STARK-based alternative could fit the same SRAM tier at production security. It can.

## Cross-ISA story, three families

Same source, same silicon, different ISA. Cortex-M33 wins overall on every proof system, but the ratio moves a lot:

| Family | RV32 / M33 | What's driving the gap |
|---|---:|---|
| STARK Fibonacci-1024 (TlsfHeap) | 1.51× | TLSF bitmap walks mispredict more on Hazard3 |
| STARK Fibonacci-1024 (BumpAlloc) | **1.21×** | **pure crypto, no allocator noise** |
| BN254 Groth16 | 1.33× | G2 scalar mul + pairing tower |
| BLS12-381 Groth16 | 2.56× | UMAAL wins big at 12-word Fp where it didn't at 8 |

Earlier baseline (pre `substrate-bn` dep-bump) had Hazard3 ~35 % *faster* on BN254 G1 scalar mul, but that went away after upstream optimization helped ARM codegen more than RISC-V. **Moral**: cross-ISA conclusions on `no_std` crypto are allocator-sensitive and library-version-sensitive. Reproducing any number here requires pinning both. Full writeup of the BLS12 case in `research/reports/2026-04-22-bls12-381-results.typ`.

## Repository layout

```
bindings/
├── crates/         Rust source (library + host tool + six firmware crates + bump alloc)
├── benchmarks/     raw + structured benchmark data, one dir per run
├── research/       Typst sources → PDFs (whitepaper, prior-art, reports, notebook)
├── web/            public site (Astro + Starlight) deployed at zkmcu.dev
└── justfile        top-level build tasks
```

### Crates

| crate | what | target |
|-------|------|--------|
| `zkmcu-verifier` | `no_std` Groth16 / BN254 verifier (EIP-197) | any |
| `zkmcu-verifier-bls12` | `no_std` Groth16 / BLS12-381 verifier (EIP-2537) | any |
| `zkmcu-verifier-stark` | `no_std` winterfell STARK verifier | any |
| `zkmcu-vectors` | `no_std` test-vector loader, all three systems | any |
| `zkmcu-bump-alloc` | `no_std` bump `GlobalAlloc` with watermark reset | any |
| `zkmcu-host-gen` | CLI, generates Groth16 (arkworks) and STARK (winter-prover) proofs | host (`std`) |
| `bench-rp2350-m33` | firmware, BN254, Cortex-M33 | `thumbv8m.main-none-eabihf` |
| `bench-rp2350-m33-bls12` | firmware, BLS12-381, Cortex-M33 | `thumbv8m.main-none-eabihf` |
| `bench-rp2350-m33-stark` | firmware, STARK, Cortex-M33 | `thumbv8m.main-none-eabihf` |
| `bench-rp2350-rv32` | firmware, BN254, Hazard3 RV32 | `riscv32imac-unknown-none-elf` |
| `bench-rp2350-rv32-bls12` | firmware, BLS12-381, Hazard3 RV32 | `riscv32imac-unknown-none-elf` |
| `bench-rp2350-rv32-stark` | firmware, STARK, Hazard3 RV32 | `riscv32imac-unknown-none-elf` |

## Build

```bash
just build                 # host crates
just test                  # arkworks ↔ substrate-bn + arkworks ↔ bls12_381 + host-side STARK
just regen-vectors         # regenerate crates/zkmcu-vectors/data/**
just build-m33             # BN254 firmware (Cortex-M33)
just build-m33-bls12       # BLS12-381 firmware (Cortex-M33)
just build-m33-stark       # STARK firmware (Cortex-M33)
just build-rv32            # BN254 firmware (Hazard3 RV32)
just build-rv32-bls12      # BLS12-381 firmware (Hazard3 RV32)
just build-rv32-stark      # STARK firmware (Hazard3 RV32)
just docs                  # Typst → research/out/*.pdf
just check-full            # fmt + clippy + tests + all six firmware builds
```

## Flashing

The Pico is connected over USB to a Raspberry Pi 5 I use as a flashing host, so the workflow is two hops. Pick whichever firmware (curve / system / ISA) you want:

1. Hold BOOTSEL on the Pico 2 W and replug. It enumerates as USB `2e8a:000f`.
2. Ship the ELF to the Pi 5 and flash:

    ```bash
    scp target/thumbv8m.main-none-eabihf/release/bench-rp2350-m33-stark pi:/tmp/bench.elf
    # on the Pi:
    picotool load -v -x -t elf /tmp/bench.elf
    ```

3. Read serial: `cat /dev/ttyACM0`, or `dd if=/dev/ttyACM0 bs=1 count=N` for a bounded capture (`stty` on the CDC can hang while a verify is in flight).

## Wire formats

Three wire formats, parallel structure, not interchangeable. Full writeup at [zkmcu.dev/wire-format](https://zkmcu.dev/wire-format/), quick reference here.

**EIP-197** (BN254, 256 B proof):

| type | size | encoding |
|------|------|----------|
| `Fq` | 32 B | big-endian, < BN254 base modulus |
| `Fr` | 32 B | big-endian, < BN254 scalar modulus (strict) |
| `G1` | 64 B | `x ‖ y`, identity = all-zeros |
| `G2` | 128 B | `x.c1 ‖ x.c0 ‖ y.c1 ‖ y.c0` |

**EIP-2537** (BLS12-381, 512 B proof):

| type | size | encoding |
|------|------|----------|
| `Fp` | 64 B | 16 zero-pad + 48 big-endian, < BLS12-381 base modulus |
| `Fr` | 32 B | big-endian, < BLS12-381 scalar modulus (strict) |
| `G1` | 128 B | `x ‖ y`, identity = all-zeros |
| `G2` | 256 B | `x.c0 ‖ x.c1 ‖ y.c0 ‖ y.c1` |

**Winterfell 0.13 `Proof::to_bytes`** (STARK, 30.9 KB at Quadratic extension): trace root + constraint root + FRI layer commitments + query proofs + OOD evals + remainder polynomial. No VK, the AIR is the verifier-side invariant. Proof size depends on AIR + trace length + blowup + queries + extension field.

**Containers** (Groth16, same shape for both curves):

- VK: `α(G1) ‖ β(G2) ‖ γ(G2) ‖ δ(G2) ‖ num_ic(u32 LE) ‖ ic[num_ic](G1)`
- Public inputs: `count(u32 LE) ‖ input[count](Fr)`

Heads up: the Fp2 byte order is `(c1, c0)` on EIP-197 and `(c0, c1)` on EIP-2537. If you're porting code between `zkmcu-verifier` and `zkmcu-verifier-bls12`, check the Fp2 order first, that's the most common place things silently go wrong. Strict canonical encoding is enforced on `Fr` for both curves, and on `Fp`-padding for EIP-2537 specifically.

## Documents

Typst sources under `research/`, build with `just docs`, output lands in `research/out/` (gitignored):

- `whitepaper.pdf`: canonical technical paper
- `prior-art.pdf`: living survey of what else exists in embedded ZK
- `2026-04-22-bls12-381-results.pdf`: BLS12-381 measurements vs frozen predictions (2 of 3 criteria fired)
- `2026-04-22-semaphore-baseline.pdf`: real-world Semaphore Groth16 proof on a Pico 2 W
- `2026-04-23-umaal-sram-groth16.pdf`: hand-written UMAAL Montgomery multiply in SRAM, 988 → 641 ms on BN254 Groth16 verify
- `2026-04-23-stark-prediction.pdf` + `stark-results.pdf`: phase 3.1, first STARK on-silicon numbers
- `2026-04-24-stark-quadratic-prediction.pdf` + `stark-quadratic-results.pdf`: phase 3.2, production-grade 95-bit STARK
- `2026-04-24-stark-variance-isolation.pdf`: phase 3.2.x, `proof.clone()` hypothesis (disconfirmed)
- `2026-04-24-stark-bump-alloc.pdf`: phase 3.2.y, silicon-baseline variance via watermark-reset bump allocator
- `2026-04-24-stark-allocator-matrix.pdf`: phase 3.2.z synthesis, production-deterministic config picked

Phase 3 paper trail (predictions frozen before measurement, results compared against the frozen file) lives under `research/notebook/`, `research/reports/`, and `benchmarks/runs/`.

## License

MIT OR Apache-2.0, pick whichever fits your project.
