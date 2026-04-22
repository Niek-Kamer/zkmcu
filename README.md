# zkmcu

`no_std` Rust verifier for Groth16 / BN254 SNARKs, small enough to run on a microcontroller. Built on top of `substrate-bn`, wire format is EIP-197 compatible so proofs generated with `arkworks` verify bit-for-bit.

First target is the Raspberry Pi Pico 2 W. RP2350 is a fun chip because it has **both** an ARM Cortex-M33 and a RISC-V Hazard3 core on the same die at the same clock, so I can run the same Rust source on both and compare without changing anything else.

## First numbers (no hand-tuning, `substrate-bn` 0.6)

Pico 2 W at 150 MHz, measured on-device with `DWT::cycle_count` on M33 and `mcycle` on RV32:

| op | Cortex-M33 | Hazard3 RV32 | RV32 / M33 |
|---|---:|---:|---:|
| G1 scalar mul (typical) | 110 ms | **72 ms** | 0.65× |
| G2 scalar mul (typical) | **210 ms** | 284 ms | 1.35× |
| pairing | **533 ms** | 707 ms | 1.33× |
| **Groth16 verify (1 public input)** | **962 ms** | 1341 ms | 1.39× |

Every iteration returns `ok=true` and iteration-to-iteration variance stays under 0.07%. Ofcourse this is a baseline, not an optimized number.

There is still a lot of room. The M33 has DSP intrinsics (`SMLAL`, `UMAAL`) for Montgomery reduction that `substrate-bn` doesn't touch, and Hazard3 has the `Zbb`/`Zba`/`Zbc` bit-manip extensions just sitting there unused. First guess is ~200 ms for M33 with intrinsics, idk if RV32 catches up but the bit-manip should close some of the pairing gap.

### Memory

Directly measured on-device (stack painting + a tracking heap wrapper):

| | Cortex-M33 | Hazard3 RV32 |
|-|---:|---:|
| `.text` | 73 KB | 72 KB |
| peak stack during verify | 15,604 B | 15,708 B |
| peak heap during verify | 81,280 B | (pending) |
| heap arena (confirmed sufficient) | **96 KB** | 256 KB |
| total RAM during verify | **~111 KB** | ~272 KB |

So the M33 build fits in ~111 KB of SRAM, wich puts it on the 128 KB tier of MCUs and secure elements (`nRF52832`, `STM32F405`, Ledger ST33, Infineon SLE78, most hardware-wallet-grade silicon). Getting into 64 KB would require either avoiding `pairing_batch` (serial pairings, ~2× verify cost) or switching to a non-pairing verifier like Nova. Not going there yet.

## Why

The thing that annoyed me into building this is that every existing "ZK on embedded" project I could find either runs under Linux on something like a Pi Zero, or it's a paper with no code. `ZPiE` (2021) is the closest published thing and it needs a full OS. For actual hardware-wallet-class devices (128 KB SRAM, no MMU, no Linux), there was nothing. So yeah I wrote it.

I also wanted to see for myself whether Cortex-M33 or Hazard3 RV32 wins on pairing-grade arithmetic. As far as I can tell nobody had published that comparison on identical silicon before. Turns out M33 wins overall by about 28% on verify, but Hazard3 wins on G1 scalar mul by ~35%. See `research/prior-art/main.typ` for the full survey.

## Repository layout

```
bindings/
├── crates/         Rust source (library + host tool + firmware)
├── benchmarks/     raw + structured benchmark data, one dir per run
├── research/       Typst sources → PDFs (whitepaper, prior-art, reports)
├── web/            public site (Next.js + Fumadocs + MDX), WIP
└── justfile        top-level build tasks
```

### Crates

| crate | what | target |
|-------|------|--------|
| `zkmcu-verifier` | `no_std` Groth16/BN254 verifier over `substrate-bn` | any |
| `zkmcu-vectors` | `no_std` test-vector loader, EIP-197 binary format | any |
| `zkmcu-host-gen` | CLI, generates Groth16 proofs via `arkworks` | host (`std`) |
| `bench-rp2350-m33` | firmware, Cortex-M33 | `thumbv8m.main-none-eabihf` |
| `bench-rp2350-rv32` | firmware, Hazard3 RV32 | `riscv32imac-unknown-none-elf` |

## Build

```bash
just build          # host crates
just test           # arkworks ↔ substrate-bn cross-check
just regen-vectors  # regenerate crates/zkmcu-vectors/data/*.bin
just build-m33      # firmware (Cortex-M33)
just build-rv32     # firmware (Hazard3 RV32)
just docs           # Typst → research/out/*.pdf
```

## Flashing

The Pico is connected over USB to a Raspberry Pi 5 I use as a flashing host, so the workflow is two hops:

1. Hold BOOTSEL on the Pico 2 W and replug. It enumerates as USB `2e8a:000f`.
2. Ship the ELF to the Pi 5 and flash:

    ```bash
    scp target/thumbv8m.main-none-eabihf/release/bench-rp2350-m33 pi:/tmp/bench-m33.elf
    # on the Pi:
    picotool load -v -x -t elf /tmp/bench-m33.elf
    ```

3. Read serial: `cat /dev/ttyACM0`, or `dd if=/dev/ttyACM0 bs=1 count=N` for a bounded capture (`stty` on the CDC can hang while a verify is in flight).

## Wire format (EIP-197 compatible)

| type | size | encoding |
|------|------|----------|
| `Fq` | 32 B | big-endian, < BN254 base modulus |
| `Fr` | 32 B | big-endian, < BN254 scalar modulus (strict) |
| `G1` | 64 B | `x ‖ y`, identity = `(0, 0)` |
| `G2` | 128 B | `x.c1 ‖ x.c0 ‖ y.c1 ‖ y.c0` |

- Verifying key: `alpha(G1) ‖ beta(G2) ‖ gamma(G2) ‖ delta(G2) ‖ num_ic(u32 LE) ‖ ic[num_ic](G1)`
- Proof: `A(G1) ‖ B(G2) ‖ C(G1)` = 256 B
- Public inputs: `count(u32 LE) ‖ input[count](Fr)`

The `Fr` strictness is intentional. `substrate-bn` silently reduces non-canonical Fr encodings mod `r`, wich is fine for pairing correctness but lets an attacker mint multiple byte-distinct-but-equivalent nullifiers. Blocked at parse time, see `SECURITY.md`.

## Documents

Typst sources under `research/`, build with `just docs`, output lands in `research/out/` (gitignored):

- `whitepaper.pdf` — canonical technical paper
- `prior-art.pdf` — living survey of what else exists in embedded ZK
- `2026-04-21-zkmcu-first-session.pdf` — master session report with the full M33 + RV32 numbers
- `2026-04-21-groth16-baseline.pdf` — tight 1-page Cortex-M33 baseline

## License

MIT OR Apache-2.0, pick whichever fits your project.
