# `result.toml` schema

Every `runs/<date>-<slug>/result.toml` must conform to this shape. Keys marked (optional) may be omitted.

```toml
[meta]
date = "2026-04-21"                  # ISO-8601 start date
target = "rp2350-cortex-m33"         # canonical target slug
toolchain = "rustc 1.94.1"
profile = "release, lto=fat, opt-level=s, codegen-units=1"
commit = "abc1234"                   # (optional) git SHA; empty string if uncommitted

[hardware]
board = "Raspberry Pi Pico 2 W"
cpu = "ARM Cortex-M33"
clock_hz = 150_000_000
sram_bytes = 524288
flash_bytes = 4194304

[libraries]                          # dependency versions that matter for the result
"substrate-bn" = "0.6.0"
"ark-groth16"  = "0.5.0"

[footprint]                          # from `size` or equivalent
text_bytes = 70500
data_bytes = 0
bss_bytes  = 262176
heap_bytes = 262144                  # configured static heap
stack_peak_bytes = 0                 # (optional) peak measured stack usage; 0 if not measured

# ---- Per-operation benchmarks. Each [bench.<name>] is a named result. ----

[bench.groth16_verify]
circuit = "x^2 = y"                  # (optional) human-readable circuit
public_inputs = 1
ic_size = 2                          # len(vk.gamma_abc_g1)
iterations = 60                      # how many samples contributed
cycles_median = 148_276_818
cycles_min    = 148_262_752
cycles_max    = 148_305_230
us_median     = 988_512
result = "ok"                        # "ok" | "rejected" | "error"

[bench.pairing]
iterations = 60
cycles_median = 80_007_766
us_median     = 533_385

[bench.g1_mul]
iterations = 60
cycles_typical = 16_500_000          # distribution is bimodal (scalar Hamming weight); use 'typical'
us_typical     = 110_000
cycles_best    = 5_100_000
us_best        = 34_000

[bench.g2_mul]
iterations = 60
cycles_typical = 31_500_000
us_typical     = 210_000
cycles_best    = 16_000_000
us_best        = 107_000
```

### Required top-level sections
- `[meta]`, `[hardware]`, `[libraries]`, `[footprint]`.

### Bench tables
- Must be under `[bench.<name>]`. `<name>` should be reproducible-stable (we'll compare across runs).
- Must contain at least `cycles_median` or `cycles_typical` **and** a matching `us_*` companion.
- If applicable, include `iterations`, `result`, and circuit-specific metadata.

### Invariants
- All `*_bytes` are exact integers.
- All `*_hz` values are integers in Hz.
- Cycle counters are always **unsigned**. Wrap-around should never appear in medians.
