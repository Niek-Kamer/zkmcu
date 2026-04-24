# 2026-04-24 — M33 STARK BabyBear + Quartic, Karatsuba extension mul

Follow-up to `2026-04-24-m33-stark-fib-1024-babybear-q`. Replaced the
16-multiplication schoolbook `ExtensibleField<4>::mul` with a 9-mult
Karatsuba + 3 sparse `mul_by_W` doublings (`W = 11 = 8 + 2 + 1`), dropping
the Mont-mul count per extension multiply from 19 to 12 (a 37 % cut).
Proof.bin is byte-identical (Karatsuba is correctness-preserving; all 30
`zkmcu-babybear` tests pass).

## Headline

**124.22 ms median**, 0.072 % std-dev variance, 18 clean iterations.
**Delta vs schoolbook: +12 µs — effectively zero, within noise.**

## The negative finding

Extension-field multiplication is not the M33 bottleneck. If it were even
20 % of verify time, a 37 % reduction in Mont-muls would show up in the
median. It didn't. Three plausible reasons, in order of my confidence:

1. LLVM at `opt-level=s + lto=fat` already CSE'd the 16-mult schoolbook
   into a form close to what Karatsuba does by hand. The 16 products
   share many common sub-expressions (e.g. `a[0] * b[0]` appears in three
   of the four `c_i` rows) and the compiler can fold them.
2. Cortex-M33's pipelined UMULL + DSP unit absorbs the extra multiplies
   via instruction-level parallelism. Mont-reduce is register-heavy but
   the pipeline tolerates it.
3. The verify-time distribution is dominated by Blake3 hashing, Merkle
   path verification, and FRI structural work, not by ExtensibleField<4>
   calls. A 37 % cut on a minority cost is proportionally smaller.

## What it rules out

The Goldilocks-vs-BabyBear gap on M33 (+66 %, 74.65 → 124 ms) is NOT
primarily about extension-multiplication count. That means further hand-
optimization of `ExtensibleField<4>::mul` (better squaring, asm UMULL
for `mont_reduce`, etc.) is unlikely to close much of the gap.

If we want BabyBear to beat Goldilocks on M33 at 95-bit conjectured
security, the lever is architectural — either a different extension
approach (a quintic extension over a 19-bit field? a 6-degree extension
over a smaller field?), a different STARK protocol (Plonky3's
circle-STARK over Mersenne-31?), or accepting a lower security target.

## What it does NOT rule out

- RV32 may still benefit from Karatsuba (it's an in-order core without
  LLVM's scheduling luxuries). See companion run
  `2026-04-24-rv32-stark-fib-1024-babybear-q-kara`.
- Variance tightened slightly (0.082 % → 0.072 %), wich suggests the
  Karatsuba code has fewer per-iteration branching decisions. Marginal
  but real.

## Reproduction

```bash
# Karatsuba is already the default in main; for reference only.
cargo run -p zkmcu-host-gen --release -- stark-babybear
just build-m33-stark-bb

scp target/thumbv8m.main-none-eabihf/release/bench-rp2350-m33-stark \
    pid-admin@10.42.0.30:/tmp/bench-m33-stark-bb.elf

# Pi 5 after BOOTSEL:
picotool load -v -x -t elf /tmp/bench-m33-stark-bb.elf
cat /dev/ttyACM0
```
