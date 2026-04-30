# zkmcu

Yeah so I've been building `no_std` Rust ZK verifiers and provers for microcontrollers. The current focus is **post-quantum Semaphore-style identity proofs on a $7 hardware token**.

Latest result: a Plonky3 STARK verifier with **dual-hash (Poseidon2 ∥ Blake3) FRI composition** runs in 1.6 s on a Cortex-M33 RP2350 with 384 KB heap. The two FRI proofs over the same statement compose: a forged proof has to fool both an algebraic hash (Poseidon2-`BabyBear`-16, audited round constants) and a generic hash (Blake3 1.8) simultaneously. Cryptanalytic surprise on either hash family does not collapse the verifier.

I did not find prior published work on either:
- a measured `no_std` Plonky3 STARK verifier running on a Cortex-M-class MCU, or
- dual-hash (Poseidon2 ∥ Blake3) FRI verification with hash-tower soundness composition.

Prior-art search at `research/notebook/2026-04-30-prior-art-stark-side.md`. If I missed something, please open an issue.

Docs site: [zkmcu.dev](https://zkmcu.dev). Reproduction: [`./reproduce.sh`](./reproduce.sh) on a dev machine + a Pico 2 W in BOOTSEL.

---

## Headline (Phase E.1, 2026-04-30)

Pico 2 W at 150 MHz, measured on-device with `DWT::cycle_count` on M33 and `mcycle` on RV32. 20 iterations per bench, range_pct < 0.1 %, every iteration `ok=true`.

| | Cortex-M33 | Hazard3 RV32 | RV32 / M33 |
|-|---:|---:|---:|
| **PQ-Semaphore d=10, dual-hash verify** | **1,611 ms** | **2,042 ms** | 1.27× |
| heap peak (drop-between pattern) | 304 KB | 304 KB | — |
| stack peak | 11.2 KB | 11.0 KB | — |
| combined proof size (P2 + B3) | 337 KB | same | — |
| security (per leg) | 127 conj. FRI + 186-bit hash floor | same | — |
| dual-hash composition | yes | yes | — |

Bench artifacts: `benchmarks/runs/2026-04-30-{m33,rv32}-pq-semaphore-dual/`.

The 1.6 s number is **full pipeline** (parse + verify both FRI legs from raw bytes). The Phase E.1 entry point parses + verifies the Poseidon2 leg, drops it, then parses + verifies the Blake3 leg, so peak heap is `max(p2_peak, b3_peak)` not the sum. 384 KB heap was sufficient; no Phase D 480 KB workaround needed. Stack peak captured cleanly.

## Phase A-E methodology

Each phase had falsifiable predictions written *before* the on-silicon bench, committed alongside `result.toml`. Four out of five phases landed outside their predicted bands. The plan, predictions, and per-phase measurements are at `bindings/.claude/plans/2026-04-29-security-128bit.md` plus the `benchmarks/runs/` directories.

| Phase | Change | M33 verify (ms) | Δ vs Phase B | Predicted band | Verdict |
|---|---|---:|---:|---|---|
| 4.0 (baseline) | BabyBear × Quartic, d=4, no grinding | 1049.72 | — | — | 95 conj. FRI |
| **A** | + 16+16 grinding bits | 1051.09 | -1.4 % | +0–8 ms | **inside band** (127 conj.) |
| **B** | + DIGEST_WIDTH 4→6 | 1065.84 | (baseline) | +12-22 % | **far below band** (1.40 %) |
| **C** | + two-stage early exit | 1130.58 | +6.1 % | reject paths < 900 ms | **far below band** (max reject 127 ms) |
| **D** (alt) | Goldilocks × Quadratic, d=4 | 1995.66 | +87.2 % | 600–680 ms band | **HYPOTHESIS REJECTED** |
| **E.1** | Phase B + Blake3 sibling FRI | 1611.39 | +51.2 % | 2200–2500 ms band | **far below band** (dual-hash composition) |

Phase D is the lead methodology bullet. We predicted Goldilocks × Quadratic to be 66 % faster than `BabyBear` × Quartic on M33 by inheriting the Phase 3.3 fib1024 finding. fib1024 is arithmetic-bound. PQ-Semaphore verify is hash-bound (64 FRI queries × ~10 Merkle hops × Poseidon2 permutations dominate the cycle budget). Phase 3.3's win did not transfer; the GL config landed +87 % slower on M33 and +112 % slower on RV32. We document the negative result rather than hide it. This is the kind of thing on-device measurement teaches you that whiteboard analysis doesn't.

## Cross-ISA story

Same die, same clock. RP2350 ships both an ARMv8-M Cortex-M33 and a RISC-V Hazard3 core in one silicon piece, wich makes the cross-ISA comparison drama-free. One Rust source tree, two target triples, same firmware loop.

| Phase | Cortex-M33 (ms) | Hazard3 RV32 (ms) | RV32 / M33 |
|---|---:|---:|---:|
| A (BB grind only) | 1051.09 | 1255.98 | 1.195 |
| B (BB d=6 + grind) | 1065.84 | 1269.73 | 1.191 |
| C (BB full pipeline) | 1130.58 | 1302.64 | 1.152 |
| D (GL × Quadratic) | 1995.66 | 2700.84 | 1.354 |
| **E.1 (dual P2 ∥ B3)** | **1611.39** | **2041.78** | **1.267** |

Phase E.1 widens cross-ISA from 1.19× to 1.27×. The Phase B Poseidon2 leg keeps its 1.19× tax (BabyBear field arithmetic feels the same on both ISAs, with UMAAL helping M33 and Hazard3 single-cycle ALU paths matching). The widening comes from the **Blake3 leg**: Hazard3 lacks single-instruction barrel rotate and emulates `x.rotate_right(n)` as `srl ; sll ; or` (3 instructions vs 1 on M33). Blake3's ARX inner loop has 8 rotates per round × 7 rounds × ~64 queries × ~10 Merkle hops, so the per-rotate cost compounds.

Per-leg estimate (dual minus Phase B baseline):
- P2 leg: ~1066 ms M33 / ~1270 ms RV32, ratio ~1.19×
- B3 leg: ~545 ms M33 / ~772 ms RV32, ratio ~1.42×

Worth flagging: **Blake3 is roughly half the cost of Poseidon2 on Cortex-M33 in the verifier role.** That inverts the usual "Poseidon2 is the embedded-friendly hash" intuition wich is correct on the prover side and wrong on the verifier side. Cortex-M33 has a 1-cycle barrel rotate so ARX schedules unroll cleanly; Poseidon2-`BabyBear` has to compute a width-16 algebraic permutation per Merkle node.

## Comparison with related work

The "MCU column is empty" observation is itself part of the contribution. None of the major STARK ecosystems (RISC Zero, Succinct SP1, Plonky3 upstream, Aztec, Powdr) publish embedded verify numbers; their published numbers are server-class only. SP1 explicitly excludes verify time from its benchmark methodology.

| Work | Year | Verifier on | Hardware | Verify scope | Verify time | PQ? |
|---|---|---|---|---|---:|---|
| Winterfell | ongoing | server | Intel i9-9980KH @ 2.4 GHz, 8c | Rescue 2^20 96-bit | 2–6 ms | yes |
| zkDilithium (ePrint 2023/414) | 2023 | server | (Winterfell defaults) | PQ anon-cred STARK | server-class | yes |
| RISC Zero zkVM | ongoing | server | r6a.16xlarge / 64 vCPU | zkVM verify | not published | yes |
| Succinct SP1 | ongoing | server | r6a.16xlarge / GPU | zkVM verify | excluded | yes |
| Plonky3 upstream | ongoing | server | x86 + AVX2/AVX-512 | various | not published | yes |
| MDPI 2024 cross-platform | 2024 | Raspberry Pi (model unspec) | ARM Cortex-A | zk-STARK (shape unspec) | 245 ms | yes |
| **this work, Phase E.1** | **2026** | **RP2350 M33 (single-core, 150 MHz, no SIMD)** | **Cortex-M33** | **PQ-Semaphore d=10, 127 conj. + dual-hash** | **1611 ms** | **yes** |
| **this work, Phase E.1** | **2026** | **RP2350 Hazard3 (single-core, 150 MHz, no SIMD)** | **RV32IMAC** | **same as above** | **2042 ms** | **yes** |

Server-class STARK verifiers run in milliseconds on AVX-equipped hardware costing $1000+. Our 1.6 s on a $7 microcontroller is in the same order of magnitude as a Pi-class Linux board running a much smaller STARK, and within 300-800× of a laptop-class i9 running a comparable Winterfell proof. The win is **not** raw verify speed — it is that the verifier fits in the power, BOM, and silicon budget of a hardware token at all, with no host or radio link assumed.

## Security notes (audit boundary)

Inherited from upstream / in-tree audit:
- Plonky3 core (`p3-uni-stark`, `p3-fri`, `p3-merkle-tree`, `p3-symmetric`, etc.) — see `vendor/Plonky3/audits/`.
- Poseidon2-`BabyBear`-16 round constants — independently audited via `crates/zkmcu-poseidon-audit` (in-tree audit crate, regenerates the constants from spec and bit-compares against Plonky3's `BABYBEAR_POSEIDON2_RC_16_*` arrays).
- Blake3 1.8 — independent upstream audits, mature widely-deployed crate.

NOT audited (the things a third-party reviewer should look at next):
- The custom PQ-Semaphore AIR (`crates/zkmcu-verifier-plonky3/src/pq_semaphore.rs`). The audited Poseidon2 constraint surface is preserved byte-for-byte; the cross-row witness-column constraints (id_col / scope_col continuity, sibling/prev_digest binding, conditional Merkle swap) are new logic, hand-checked but not externally audited.
- The postcard wire format and proof parsing.
- The Blake3-flavoured StarkConfig wiring (`pq_semaphore_blake3.rs`, `pq_semaphore_dual.rs`).
- The firmware (allocator integration, USB CDC-ACM transport, panic-halt, bench harness).

Side channels: timing analysis in-progress via `crates/bench-rp2350-m33-timing-oracle`. Power, fault, EM not analyzed. Pico 2 W has no secure element, no tamper detection. An attacker with ~30 minutes of physical access to a powered device can dump SRAM via SWD or BOOTSEL and recover any keys or witness material in RAM. **Honest label: convenience-grade self-custody on open hardware, not Ledger-grade tamper resistance.** The dual-hash STARK soundness composition does not change the physical-tamper picture.

Full security analysis at `research/notebook/2026-04-30-security-claim-table.md`.

---

## Earlier work in this workspace

The repo also contains earlier ZK-on-MCU work that landed before the PQ-Semaphore arc. These are not the headline of the current writeup but they are real measurements with their own bench artifacts. Each gets its own writeup at some point.

### Groth16 / BN254 verifier (the SNARK baseline that PQ-Semaphore replaces)

`zkmcu-verifier` is a `no_std` Groth16 verifier in EIP-197 wire format using `substrate-bn`. Stock crates.io baseline lands at 988 ms on Cortex-M33; the same verify with hand-rolled ARMv8-M UMAAL Montgomery multiply asm in `vendor/bn` (forked, behind a `cortex-m33-asm` feature flag) lands at **641 ms**. The asm path is differentially tested via a three-implementation chain (`mul_reduce` asm vs `mul_reduce_u32_ref` portable u32 SOS on-device, `mul_reduce_rust` u128 Montgomery vs `mul_reduce_u32_ref` host-side cargo test); the selftest is a separate firmware flash, the headline 641 ms bench does not re-run it at boot.

A real Semaphore v4.14.2 depth-10 Groth16 proof generated by snarkjs verifies through `zkmcu-verifier` in 761 ms (M33, with UMAAL asm). Same VK + proof bytes the Ethereum Semaphore precompile accepts, running unmodified on a $7 MCU. This is the SNARK baseline against wich the PQ-Semaphore Phase A-E numbers compete.

Reports: `research/reports/2026-04-22-semaphore-baseline.typ`, `research/reports/2026-04-23-umaal-sram-groth16.typ`.

### Groth16 / BLS12-381 verifier (sibling baseline, EIP-2537)

`zkmcu-verifier-bls12` proves the approach generalizes across curves. 2,015 ms M33 / 5,151 ms RV32. Cross-ISA gap is wider than BN254 (2.56× vs 1.39×) because the 12-word Fp Montgomery multiply benefits more from UMAAL than BN254's 8-word Fp does, and `bls12_381` doesn't use UMAAL. Report: `research/reports/2026-04-22-bls12-381-results.typ`.

### Winterfell STARK threshold-check prover

`zkmcu-verifier-stark` is the Winterfell sibling to the Plonky3 verifier. It also includes a host-driven prover that runs `no_std` on the firmware: 48 ms prove + 50 ms verify, 78 KB heap, on a real `value < threshold` AIR for IoT sensor attestation. Report: `research/reports/2026-04-24-stark-quadratic-results.typ`. Separate writeup planned, framed at the embedded sensor / industrial attestation audience rather than the ZK identity audience.

### Allocator-determinism methodology finding

In the course of measuring the STARK verifier we found the stock `LlffHeap` (linked-list first-fit) allocator was responsible for most of the timing variance (5-10× silicon noise floor). Switching to `embedded-alloc::TlsfHeap` recovered silicon-baseline variance at the cost of ~5 ms median verify. We also wrote a custom watermark-reset bump allocator (`zkmcu-bump-alloc`) as a measurement tool; it confirms the crypto itself is deterministic and that allocator choice can swing the M33-vs-Hazard3 ratio by 30 %. Report: `research/reports/2026-04-24-stark-allocator-matrix.typ`.

This finding has implications for any cross-ISA `no_std` crypto benchmark: allocator-sensitive workloads measure the allocator, not the workload, unless you report which one you used.

---

## Repository layout

```
bindings/
├── crates/         Rust source (verifiers + provers + audit + firmware crates)
├── benchmarks/     raw + structured benchmark data, one dir per run
├── research/       Typst sources → PDFs (whitepaper, prior-art, reports, notebook)
├── vendor/         third-party path deps (Plonky3, substrate-bn fork, winterfell)
├── reproduce.sh    one-shot dev-machine reproduction script
└── justfile        top-level build tasks
```

Web tree at `../web/` (sibling repo, separate deploy at zkmcu.dev).

### Crates (current focus first, earlier work below)

| crate | what | target |
|---|---|---|
| `zkmcu-verifier-plonky3` | `no_std` Plonky3 STARK verifier (PQ-Semaphore custom AIR + Goldilocks alt + Blake3 sibling + dual entry point) | any |
| `zkmcu-poseidon-audit` | in-tree audit of Poseidon2-`BabyBear` and Poseidon2-Goldilocks round constants | host |
| `bench-rp2350-m33-pq-semaphore-dual` | Phase E.1 dual-hash bench, Cortex-M33 | `thumbv8m.main-none-eabihf` |
| `bench-rp2350-rv32-pq-semaphore-dual` | Phase E.1 dual-hash bench, Hazard3 RV32 | `riscv32imac-unknown-none-elf` |
| `bench-rp2350-{m33,rv32}-pq-semaphore` | Phase B BabyBear-d6 baseline benches | both ISAs |
| `bench-rp2350-{m33,rv32}-pq-semaphore-gl` | Phase D Goldilocks × Quadratic alt-config benches | both ISAs |
| `bench-rp2350-{m33,rv32}-pq-semaphore-reject` | Phase C two-stage early-exit benches | both ISAs |
| `zkmcu-verifier` | `no_std` Groth16 / BN254 verifier (EIP-197) | any |
| `zkmcu-verifier-bls12` | `no_std` Groth16 / BLS12-381 verifier (EIP-2537) | any |
| `zkmcu-verifier-stark` | `no_std` Winterfell STARK verifier + AIRs (Phase 3.x) | any |
| `zkmcu-vectors` | `no_std` test-vector loader, all proof systems | any |
| `zkmcu-bump-alloc` | `no_std` bump `GlobalAlloc` with watermark reset (measurement tool, not production) | any |
| `zkmcu-host-gen` | CLI, generates Groth16 (arkworks) + STARK (Winterfell + Plonky3) test vectors | host (`std`) |
| `bench-rp2350-m33`, `bench-rp2350-rv32` | BN254 firmware, both ISAs | both |
| `bench-rp2350-{m33,rv32}-bls12` | BLS12-381 firmware, both ISAs | both |
| `bench-rp2350-{m33,rv32}-stark`, `*-stark-prover-*` | Winterfell firmware (Phase 3.x) | both |
| `bench-rp2350-m33-bn-asm-test` | UMAAL asm differential-test firmware | M33 |
| `bench-rp2350-m33-timing-oracle` | timing-side-channel analysis bench (in progress) | M33 |
| `bench-core` | shared firmware infrastructure (USB, allocator, cycle counter) | both |

## Build

```bash
just check                  # fmt + clippy + host tests on every crate
just regen-vectors          # regenerate crates/zkmcu-vectors/data/**
just build-m33-pq-semaphore-dual    # Phase E.1 firmware (Cortex-M33)
just build-rv32-pq-semaphore-dual   # Phase E.1 firmware (Hazard3 RV32)
just docs                   # Typst → research/out/*.pdf
just check-full             # check + every firmware build
```

Or just run [`./reproduce.sh`](./reproduce.sh) which does check + regen + build of both Phase E.1 firmware images and prints the exact picotool + `cat /dev/ttyACM0` block to run on your flashing host.

## Flashing

The Pico is connected over USB to a flashing host (a Raspberry Pi 5 in our setup). Put the Pico in BOOTSEL manually, then:

```bash
# dev machine:
scp target/thumbv8m.main-none-eabihf/release/bench-rp2350-m33-pq-semaphore-dual pi:/tmp/bench.elf

# on the flashing host:
picotool load -v -x -t elf /tmp/bench.elf
cat /dev/ttyACM0
```

`stty` on the CDC device can hang while a long crypto call is in flight, avoid it. Use `dd if=/dev/ttyACM0 bs=1 count=N` for a bounded capture.

## Wire formats

Three wire formats live in this repo, not interchangeable. Quick reference:

**Plonky3 STARK proof** (PQ-Semaphore d=10 dual): postcard-encoded `Proof<StarkConfig>`. Phase E.1 ships two proofs (172.9 KB Poseidon2 leg + 163.8 KB Blake3 leg) plus 96 B public inputs (24 × 4-byte `BabyBear` elements: merkle_root, nullifier, signal_hash, scope_hash). Verifier-side `MAX_PROOF_SIZE` = 320 KB length cap.

**EIP-197** (BN254 Groth16, 256 B proof):

| type | size | encoding |
|---|---|---|
| `Fq` | 32 B | big-endian, < BN254 base modulus |
| `Fr` | 32 B | big-endian, < BN254 scalar modulus (strict) |
| `G1` | 64 B | `x ‖ y`, identity = all-zeros |
| `G2` | 128 B | `x.c1 ‖ x.c0 ‖ y.c1 ‖ y.c0` |

**EIP-2537** (BLS12-381 Groth16, 512 B proof):

| type | size | encoding |
|---|---|---|
| `Fp` | 64 B | 16 zero-pad + 48 big-endian, < BLS12-381 base modulus |
| `Fr` | 32 B | big-endian, < BLS12-381 scalar modulus (strict) |
| `G1` | 128 B | `x ‖ y`, identity = all-zeros |
| `G2` | 256 B | `x.c0 ‖ x.c1 ‖ y.c0 ‖ y.c1` |

Heads up: the Fp2 byte order is `(c1, c0)` on EIP-197 and `(c0, c1)` on EIP-2537. If you're porting between `zkmcu-verifier` and `zkmcu-verifier-bls12`, check the Fp2 order first, thats the most common place things silently break.

## Documents

Typst sources under `research/`, build with `just docs`, output in `research/out/` (gitignored). The canonical writeup for the current PQ-Semaphore A-E arc is in flight; the in-tree predecessors:

- `whitepaper.pdf` — canonical technical paper (multi-track)
- `prior-art.pdf` — prior-art survey (Groth16 side; STARK side at `research/notebook/2026-04-30-prior-art-stark-side.md`)
- `2026-04-22-bls12-381-results.pdf` — BLS12-381 measurements vs frozen predictions
- `2026-04-22-semaphore-baseline.pdf` — real-world Semaphore v4 Groth16 proof on Pico 2 W
- `2026-04-23-umaal-sram-groth16.pdf` — hand-written UMAAL Montgomery multiply, 988 → 641 ms
- `2026-04-24-stark-allocator-matrix.pdf` — allocator comparison, production config picked

Phase A-E paper trail lives under `research/notebook/2026-04-{29,30}-*` and `benchmarks/runs/2026-04-{29,30}-*`.

## What this is NOT

- NOT a hardware wallet. No secure element, no tamper resistance, no key custody. Convenience-grade self-custody on open hardware.
- NOT a prover (yet). The Plonky3 PQ-Semaphore prover is host-side `std`, runs on a laptop. The on-device 48 ms prover work is Winterfell + a different AIR + different security parameters; that's a separate writeup at some point.
- NOT a deployable Semaphore replacement. The custom AIR has not been externally audited; integration with the Semaphore Protocol's identity commitment scheme is future work.
- NOT a claim that Poseidon2 is broken. Both legs of the dual-hash composition are individually trusted; the dual structure is defence in depth, not a vote of no-confidence in either hash.

## Citation

If this work is useful to your research, please cite:

```
@misc{kamer2026pqsemaphore,
  author = {Niek Kamer},
  title  = {Post-quantum Semaphore on a \$7 microcontroller in 1.6 seconds:
            a benchmark of Plonky3 STARK verification with dual-hash composition
            on Cortex-M33 and Hazard3 RV32},
  year   = {2026},
  url    = {https://zkmcu.dev/research/2026-04-30-pq-semaphore-128bit/},
  note   = {Source: \url{https://github.com/Niek-Kamer/zkmcu}}
}
```

ePrint URL added once the report is filed.

## License

MIT OR Apache-2.0.
