#!/usr/bin/env bash
#
# reproduce.sh — one-shot reproducibility for the headline PQ-Semaphore
#                dual-hash STARK verify number on the Raspberry Pi Pico 2 W.
#
# Headline: 1611 ms on Cortex-M33, 2042 ms on Hazard3 RV32, 384 KB heap,
# 337 KB combined proof bytes, all 20 iters ok=true.
# Source of truth: benchmarks/runs/2026-04-30-{m33,rv32}-pq-semaphore-dual/result.toml
#
# What this script does on the dev machine:
#   1. Verify toolchain (rustc, cargo, just, picotool optional)
#   2. Run `just check` (fmt + clippy + host tests)
#   3. Regenerate the dual-proof test vectors from witness
#   4. Build M33 firmware (cross-compile thumbv8m.main-none-eabihf)
#   5. Build RV32 firmware (cross-compile riscv32imac-unknown-none-elf)
#   6. Print the exact picotool + cat /dev/ttyACM0 commands the user
#      runs on the Pi 5 to capture the bench
#
# What this script does NOT do automatically:
#   - Flash the Pico (requires manual BOOTSEL each time, see CLAUDE.md)
#   - SSH into the Pi 5 (per repo policy, hand the user copy-paste)
#   - Capture serial (run `cat /dev/ttyACM0` yourself; the script tells
#     you exactly which command, which device, and how many iters to
#     wait for)
#
# Tested on: Manjaro Linux 6.18 + rustup 1.94.1 + just 1.x + picotool 2.x
#
# Time on a modern dev machine: ~3 minutes (cold cache, rust toolchain
# already installed). Add ~5 minutes if Plonky3 vendored crates rebuild.

set -euo pipefail

# Directory of this script — always cd into the repo root.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

C_BLUE="\033[1;34m"
C_GREEN="\033[1;32m"
C_YELLOW="\033[1;33m"
C_RED="\033[1;31m"
C_RESET="\033[0m"

step() { printf "${C_BLUE}==>${C_RESET} %s\n" "$1"; }
ok()   { printf "${C_GREEN}OK${C_RESET}  %s\n" "$1"; }
warn() { printf "${C_YELLOW}!${C_RESET}   %s\n" "$1"; }
err()  { printf "${C_RED}FAIL${C_RESET} %s\n" "$1" >&2; }

# ---------- 1. toolchain check ----------
step "Checking toolchain"
command -v rustc >/dev/null    || { err "rustc not found. Install via https://rustup.rs/"; exit 1; }
command -v cargo >/dev/null    || { err "cargo not found.";  exit 1; }
command -v just  >/dev/null    || { err "just not found. Install: cargo install just"; exit 1; }
command -v rustup >/dev/null   || { err "rustup not found.";  exit 1; }

rustup target add thumbv8m.main-none-eabihf >/dev/null 2>&1 || true
rustup target add riscv32imac-unknown-none-elf >/dev/null 2>&1 || true

ok "rustc $(rustc --version | awk '{print $2}'), cross-targets installed"

# ---------- 2. just check (fmt + clippy + tests) ----------
step "Running 'just check' (fmt + clippy + host tests)"
if just check; then
    ok "all host checks passed"
else
    err "just check failed — repo is not in a clean state. Aborting."
    exit 1
fi

# ---------- 3. regenerate test vectors ----------
step "Regenerating dual-proof test vectors"
cargo run -p zkmcu-host-gen --release -- pq-semaphore-dual >/dev/null
DUAL_DIR="crates/zkmcu-vectors/data/pq-semaphore-d10-dual"
P2_BYTES=$(stat -c %s "$DUAL_DIR/proof_p2.bin")
B3_BYTES=$(stat -c %s "$DUAL_DIR/proof_b3.bin")
PUB_BYTES=$(stat -c %s "$DUAL_DIR/public.bin")
ok "proof_p2 = $P2_BYTES B, proof_b3 = $B3_BYTES B, public = $PUB_BYTES B"
# Sanity: combined proof should be ~337 KB (172_977 + 163_824 = 336_801 in our reference run)
if [ "$P2_BYTES" -lt 100000 ] || [ "$B3_BYTES" -lt 100000 ]; then
    warn "proof sizes unexpectedly small — verify your toolchain matches the reference"
fi

# ---------- 4. build M33 firmware ----------
step "Building Cortex-M33 firmware (bench-rp2350-m33-pq-semaphore-dual)"
just build-m33-pq-semaphore-dual >/dev/null
M33_ELF="target/thumbv8m.main-none-eabihf/release/bench-rp2350-m33-pq-semaphore-dual"
[ -f "$M33_ELF" ] || { err "M33 ELF not produced at $M33_ELF"; exit 1; }
ok "M33 ELF: $M33_ELF ($(stat -c %s "$M33_ELF") bytes)"

# ---------- 5. build RV32 firmware ----------
step "Building Hazard3 RV32 firmware (bench-rp2350-rv32-pq-semaphore-dual)"
just build-rv32-pq-semaphore-dual >/dev/null
RV32_ELF="target/riscv32imac-unknown-none-elf/release/bench-rp2350-rv32-pq-semaphore-dual"
[ -f "$RV32_ELF" ] || { err "RV32 ELF not produced at $RV32_ELF"; exit 1; }
ok "RV32 ELF: $RV32_ELF ($(stat -c %s "$RV32_ELF") bytes)"

# ---------- 6. print the manual flash + capture instructions ----------
cat <<EOF

${C_GREEN}=== Build complete. Now flash + capture on hardware. ===${C_RESET}

The headline result requires a Raspberry Pi Pico 2 W (RP2350) connected
to a host that has 'picotool' installed. The user manually puts the
Pico into BOOTSEL mode (hold BOOTSEL while plugging in or while
pressing RESET if your board has a reset button).

Reference set-up (per repo conventions): a Raspberry Pi 5 hosts the
Pico, this dev machine SCPs the ELF over and the user runs picotool.
You can substitute any host with a USB port + picotool.

${C_BLUE}--- Cortex-M33 capture ---${C_RESET}

  # On the dev machine:
  scp $M33_ELF \\
      <pi-host>:/tmp/bench-m33-dual.elf

  # On the Pi (or whichever host has the Pico in BOOTSEL):
  picotool load -v -x -t elf /tmp/bench-m33-dual.elf
  cat /dev/ttyACM0 | tee /tmp/m33-dual-raw.log

  # Capture at least 25 lines (boot + boot_measure + 20 iterations + 1
  # extra). Press Ctrl+C in the cat. Reference iter shape:
  #   [N] pq_semaphore_dual_verify: cycles=241_708_178 us=1_611_387 ms=1611 heap_peak=304180 ok=true
  # Headline: ms_median = 1611, range_pct < 0.1%, all iters ok=true.

${C_BLUE}--- Hazard3 RV32 capture ---${C_RESET}

  scp $RV32_ELF \\
      <pi-host>:/tmp/bench-rv32-dual.elf

  picotool load -v -x -t elf /tmp/bench-rv32-dual.elf
  cat /dev/ttyACM0 | tee /tmp/rv32-dual-raw.log

  # Headline: ms_median = 2041, range_pct < 0.1%, all iters ok=true.

${C_GREEN}--- Reference numbers ---${C_RESET}

  Cortex-M33:  benchmarks/runs/2026-04-30-m33-pq-semaphore-dual/result.toml
  Hazard3:     benchmarks/runs/2026-04-30-rv32-pq-semaphore-dual/result.toml

  Each result.toml has the median, min, max, cycles_*, range_pct, the
  full security analysis, and predictions vs measured deltas.

  Your raw.log should match the per-iter shape exactly. If your
  ms_median differs by more than ~5 ms, either the toolchain or the
  vectors regenerated to a different seed (re-check step 3).

${C_BLUE}--- Phase A-E predecessor benches ---${C_RESET}

  If you want to walk the full five-phase methodology, the per-phase
  bench artifacts are at:

  Phase A (grinding):  benchmarks/runs/2026-04-29-{m33,rv32}-pq-semaphore-grind32/
  Phase B (digest=6):  benchmarks/runs/2026-04-29-{m33,rv32}-pq-semaphore-d6/
  Phase C (early-exit): benchmarks/runs/2026-04-29-{m33,rv32}-pq-semaphore-reject/
  Phase D (Goldilocks): benchmarks/runs/2026-04-29-{m33,rv32}-pq-semaphore-gl/
  Phase E.1 (dual):    benchmarks/runs/2026-04-30-{m33,rv32}-pq-semaphore-dual/

  Each of these has its own firmware crate under crates/bench-rp2350-*-*
  and corresponding 'just build-*' / 'just lint-*' targets. The phase
  D writeup at benchmarks/runs/2026-04-29-m33-pq-semaphore-gl/notes.md
  is the lead-bullet "hypothesis rejected" methodology paragraph.

EOF

ok "reproduce.sh complete. Hand the flash + capture block to the user."
