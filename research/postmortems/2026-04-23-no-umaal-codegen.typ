#import "/research/lib/template.typ": *

#show: paper.with(
  title: "LLVM emits zero UMAAL on thumbv8m.main",
  authors: ("zkmcu",),
  date: "2026-04-23",
  kind: "postmortem",
  abstract: [
    Disassembly audit of the 988 ms Groth16/BN254 baseline firmware. Goal: figure out whether any remaining compiler-flag lever could close the gap to prior-art MCU pairings, or whether the gap is LLVM instruction selection. Result: 145 UMULL + UMLAL across the whole binary, zero UMAAL. The only way to get UMAAL emitted on this target is hand-written asm, wich requires forking `substrate-bn`.
  ],
)

= What I measured

From `llvm-objdump` on the opt=s release build of `bench-rp2350-m33`:

- *Zero UMAAL instructions in the entire binary.* 145 UMULL + UMLAL total, 140 of wich are inside `substrate_bn::arith::U256::mul` alone. LLVM never pattern-matches our `u128`-based schoolbook multiplication to UMAAL on thumbv8m.main. UMAAL does `a*b + c + d → 64-bit` in one cycle; what we get instead is UMULL + ADDS + ADCS chains at 3-4 cycles per equivalent step. This matches what pqm4 and BearSSL document for why they hand-roll Fp arithmetic in asm.
- *Heavy register spilling confirmed.* `U256::mul` = 4116 bytes with 226 loads and 162 stores around ~140 wide multiplications. 32-bit ARM has 12 usable GPRs; 256-bit state wants 16+ slots; the compiler is reloading operands on almost every partial product.
- *No external compiler-builtins stubs in the crypto hot path.* No `__aeabi_lmul`, `__multi3`, `__umulsidi3` or similar. u128 multiplications are fully inlined. Only `__rust_specialized_div_rem::u64_div_rem`, `memcpy`, `memmove` show up, none of wich are in Montgomery arithmetic.

= Why compiler flags can't fix this

Rebuilding with `-Z build-std=core,compiler_builtins` at `target-cpu=cortex-m33` is dead because there is no external helper to recompile. There is also no `target-feature=+umaal` lever because UMAAL is mandatory on ARMv7-M DSP / ARMv8-M mainline, so rustc's view is "always enabled", the gap is purely in instruction selection.

The prior opt-level experiment already confirmed the broader point: flag tweaks don't move this baseline by more than a few percent and can easily regress (see 2026-04-23-opt-level-3-regression). The real levers are not in rustc.

= Live levers, in order

+ *RAM-linked hot `.text`* to sidestep the RP2350's 16 KB XIP cache. Independent of any `substrate-bn` change, one linker-script experiment. Addresses the cache-pressure hypothesis that fell out of the opt=3 regression.
+ *Fork `substrate-bn`, rewrite `U256::mul` (and Montgomery reduction) in hand-written ARMv8-M asm using UMAAL.* Biggest theoretical win per prior art (roughly 2× on Fp, 1.5-1.8× on verify). Multi-session effort. Requires constant-time discipline.
+ *GLV endomorphism on G1 MSM* and *tuned final-exponentiation chain* (Fuentes-Castañeda or Duquesne-Ghammam). Each 10-30 %. Both need the fork.

Compiler-flag experiments are not on this list anymore.

= Bonus observation

`substrate_bn::groups::AffineG<G2Params>::precompute` at 1504 bytes is present in the LTO-fat binary and called by `pairing_batch`. G2 line-coefficient precomputation is already done internally. That lever is claimed, not a future opportunity.

= Reproducibility

```bash
RUST_BIN="$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | grep '^host:' | cut -d' ' -f2)/bin"
BIN=target/thumbv8m.main-none-eabihf/release/bench-rp2350-m33

# UMAAL count: expected 0
"$RUST_BIN/llvm-objdump" -d --no-show-raw-insn "$BIN" | awk '$2 ~ /umaal/' | wc -l

# UMULL + UMLAL count: expected ~145
"$RUST_BIN/llvm-objdump" -d --no-show-raw-insn "$BIN" \
  | awk 'NF>=2 && $1 ~ /^[0-9a-f]+:$/ {print $2}' \
  | grep -cE '^umull|^umlal'
```

The symbol hash on `U256::mul` will drift on recompile. Re-look it up with `llvm-nm --print-size --size-sort` and grep for `U256.*mul` before disassembling.

= Status

This postmortem closed when UMAAL asm landed under `vendor/bn/` behind the `cortex-m33-asm` feature, dropping Groth16 verify from 988 ms to 641 ms. Full arc: `research/reports/2026-04-23-umaal-sram-groth16.typ`.
