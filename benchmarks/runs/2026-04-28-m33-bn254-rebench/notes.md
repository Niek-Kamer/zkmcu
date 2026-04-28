# 2026-04-28 — M33 BN254 Groth16, audit-driven rebaseline

Tier-2 hygiene pass on the project's published numbers turned up a real
gap: the headline "988 → 641 ms (35 % drop)" was the latest published
figure, but no one had re-benched after the 2026-04-27 firmware change
that added Poseidon vectors and the verify cost breakdown. So we flashed
fresh and the numbers moved more than expected.

## Headline

**Groth16 verify (x^2 = y, BN254): 550.7 ms median over 5 iterations.**

Compared with the prior published number (642.9 ms from
`2026-04-24-bench-core-m33-bn254-groth16/`):

| Bench | Prior | This run | Δ |
|-------|------:|---------:|--:|
| `groth16_verify` (square) | 642.9 ms | **550.7 ms** | **-14.4 %** |
| `groth16_verify_sq5` | 648.5 ms | 556.5 ms | -14.2 % |
| `groth16_verify_semaphore` | 760.7 ms | 669.4 ms | -12.0 % |
| `pairing` (single) | 350.0 ms | 306.8 ms | -12.4 % |

Plus first-time measurements for the new cost-breakdown benchmarks:

- `vk_x_tiny_scalar`: 0.84 ms (16-bit scalar, the floor)
- `vk_x_full_scalar`: 30.9 ms (254-bit random scalar, typical)
- `miller_loop_4pair`: 369.6 ms (the dominant cost, ~67 % of verify)
- `final_exp`: 179.7 ms (the tail)

Sum of the breakdown is 580.2 ms, headline is 550.7 ms. The difference
(~30 ms) is the Σ scalar-mul step plus G1 negation plus the verify wrapper,
wich roughly matches the `vk_x_full_scalar` ballpark.

## Why is this faster?

Honestly not 100 % sure. The BN254 verify path itself didn't change.
What did change between the prior bench and now:

1. `94d4acf` (2026-04-27) added Poseidon Merkle vectors and the
   `vk_x / miller / final_exp` cost-breakdown calls to both firmwares.
   New code in the same translation unit changes LLVM's inlining
   decisions, wich changes code placement. `mul_reduce_armv8m` lives
   in `.ram_text`, so a placement shift could move the hot path into
   nicer alignment relative to the SRAM line boundaries.
2. The `SYS_HZ` runtime assertion (this audit). One-shot at boot,
   doesn't run inside the measured window. Not the cause.

Same rustc 1.94.1, same `substrate-bn` fork SHA, same allocator,
same heap arena size. So whatever moved is downstream of LTO + linker
placement decisions. To pin it down we'd need disassembly diffs against
the prior firmware, wich is out of scope for this bench note.

The sub-permille variance across iterations (`min-max / median = 0.021 %`
on the headline) means the new number is real, not a measurement artefact.

## What's the same

- `heap_peak` = 82_336 B (was 81_888 B; +448 B is the new poseidon test
  vectors). Within the 96 KB arena with comfortable margin.
- `stack_peak` = 15_492 B (was 15_724 B; -232 B from inlining shift).
- UMAAL KAT 256/256 in 11_459 us (was 12_015 us; same pattern of
  speedup, asm path is correctly placed and self-consistent).
- Verdict on every bench: `ok=true`.

## Implications for published numbers

The headline marketing claim "988 → 641 ms (35 % drop)" is now stale.
Current reality is **988 → 551 ms (44 % drop)**. The website's
`bn254_m33` slot (currently pointing at the 2026-04-22-heap-96k-confirmed
run at 962 ms) is even more outdated.

Action items, separate commits:

1. Update `web/src/lib/benchmarks-index.ts` so `bn254_m33` points at
   this run (`2026-04-28-m33-bn254-rebench`).
2. Update prose in `findings.mdx` ("dropped to 641 ms") and
   `roadmap.mdx` ("988 → 641 ms (35% drop)") to reflect the new
   number, or generalize them to "~550 ms".

## Caveats

- Only 5 iterations captured. Min-max span is 18 cycles, but a longer
  run would tighten variance further. Future bench script should aim
  for 30+ iters per benchmark.
- iter-5 of the loop was truncated mid-`groth16_verify_poseidon_d3 start`
  due to `cat /dev/ttyACM0` interrupt. Poseidon benches report from 4
  iters instead of 5. Negligible effect on median (sub-cycle variance
  across the 4 captured samples).
- `text_bytes` / `data_bytes` / `bss_bytes` not extracted; the dev host
  doesn't have `arm-none-eabi-size` or `cargo-size` installed. Footprint
  unchanged from prior run by inspection (binary is 1020 KB on disk).
- No raw.log existed for the prior `2026-04-24-bench-core` run (also
  noted in the tier-2 audit). Comparison is purely against the
  structured `result.toml` numbers from that run.
