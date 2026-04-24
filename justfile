# zkmcu — top-level task runner. `just <target>` from repo root.

default:
    @just --list

# ---- Rust ---------------------------------------------------------------

# Build all host crates (verifier, vectors, host-gen).
build:
    cargo build --release

# Build the firmware for the Pico 2 W (Cortex-M33, BN254).
build-m33:
    cd crates/bench-rp2350-m33 && cargo build --release

# Build the firmware for the Pico 2 W (Cortex-M33, BLS12-381).
build-m33-bls12:
    cd crates/bench-rp2350-m33-bls12 && cargo build --release

# Build the firmware for the Pico 2 W (Hazard3 RV32, BN254).
build-rv32:
    cd crates/bench-rp2350-rv32 && cargo build --release

# Build the firmware for the Pico 2 W (Hazard3 RV32, BLS12-381).
build-rv32-bls12:
    cd crates/bench-rp2350-rv32-bls12 && cargo build --release

# Build the firmware for the Pico 2 W (Cortex-M33, STARK Fibonacci, Goldilocks baseline).
build-m33-stark:
    cd crates/bench-rp2350-m33-stark && cargo build --release

# Build the Cortex-M33 STARK firmware with the BabyBear + Quartic fork (phase 3.3).
build-m33-stark-bb:
    cd crates/bench-rp2350-m33-stark && cargo build --release --features babybear

# Build the firmware for the Pico 2 W (Hazard3 RV32, STARK Fibonacci, Goldilocks baseline).
build-rv32-stark:
    cd crates/bench-rp2350-rv32-stark && cargo build --release

# Build the Hazard3 RV32 STARK firmware with the BabyBear + Quartic fork (phase 3.3).
build-rv32-stark-bb:
    cd crates/bench-rp2350-rv32-stark && cargo build --release --features babybear

# Run every native test (cross-check: arkworks <-> substrate-bn).
test:
    cargo test --release

# Check formatting (does not modify files).
fmt-check:
    cargo fmt --all --check

# Format every crate in the workspace.
fmt:
    cargo fmt --all

# Clippy at -D warnings. Host crates first (default-members), then each firmware
# crate separately against its own target.
lint: lint-host lint-m33 lint-m33-bls12 lint-m33-stark lint-m33-stark-bb lint-rv32 lint-rv32-bls12 lint-rv32-stark lint-rv32-stark-bb

lint-host:
    cargo clippy --all-targets --release -- -D warnings

lint-m33:
    cd crates/bench-rp2350-m33 && cargo clippy --release -- -D warnings

lint-m33-bls12:
    cd crates/bench-rp2350-m33-bls12 && cargo clippy --release -- -D warnings

lint-rv32:
    cd crates/bench-rp2350-rv32 && cargo clippy --release -- -D warnings

lint-rv32-bls12:
    cd crates/bench-rp2350-rv32-bls12 && cargo clippy --release -- -D warnings

lint-m33-stark:
    cd crates/bench-rp2350-m33-stark && cargo clippy --release -- -D warnings

lint-m33-stark-bb:
    cd crates/bench-rp2350-m33-stark && cargo clippy --release --features babybear -- -D warnings

lint-rv32-stark:
    cd crates/bench-rp2350-rv32-stark && cargo clippy --release -- -D warnings

lint-rv32-stark-bb:
    cd crates/bench-rp2350-rv32-stark && cargo clippy --release --features babybear -- -D warnings

# Everything that must pass before a commit.
check: fmt-check lint test

# Full gate including every firmware build, used before cutting a benchmark run.
check-full: check build-m33 build-m33-bls12 build-m33-stark build-m33-stark-bb build-rv32 build-rv32-bls12 build-rv32-stark build-rv32-stark-bb

# Regenerate the committed test vectors.
regen-vectors:
    cargo run -p zkmcu-host-gen --release

# Report outdated dependencies. Requires `cargo install cargo-outdated` once.
outdated:
    cargo outdated --workspace --root-deps-only

# ---- Fuzzing (cargo-fuzz + libFuzzer) ---------------------------------
#
# The `fuzz/` crate is workspace-excluded. cargo-fuzz needs nightly for
# the sanitizer rustflags it passes (ASan by default). Targets start from
# the committed seed corpus under `fuzz/seeds/<target>/`.
#
# Default run length is 60 seconds per target. Pass an int to override:
#   just fuzz-bn254-parse-vk 300

FUZZ_SECS := "60"

# Run a single target. First positional arg is the target name (must match
# a `[[bin]]` in fuzz/Cargo.toml); second is the wall-clock time in seconds.
fuzz TARGET SECS=FUZZ_SECS:
    cd fuzz && cargo +nightly fuzz run {{TARGET}} seeds/{{TARGET}}/ -- -max_total_time={{SECS}}

# Run every target for SECS seconds each, in sequence. Stops on first crash.
# Use for a quick "did anything regress" pass before committing.
fuzz-smoke SECS="10":
    just fuzz bn254_parse_vk {{SECS}}
    just fuzz bn254_parse_proof {{SECS}}
    just fuzz bn254_parse_public {{SECS}}
    just fuzz bn254_verify_bytes {{SECS}}
    just fuzz bls12_parse_vk {{SECS}}
    just fuzz bls12_parse_proof {{SECS}}
    just fuzz bls12_parse_public {{SECS}}
    just fuzz stark_parse_proof {{SECS}}

# List fuzz targets defined in fuzz/Cargo.toml.
fuzz-list:
    cd fuzz && cargo +nightly fuzz list

# ---- Research PDFs ------------------------------------------------------

# Output directory for compiled Typst PDFs (gitignored).
out := "research/out"

# Build every Typst document under /research into research/out/*.pdf.
# --root . lets each doc use absolute imports like "/research/lib/template.typ".
docs:
    mkdir -p {{out}}
    typst compile --root . research/prior-art/main.typ   {{out}}/prior-art.pdf
    typst compile --root . research/whitepaper/main.typ  {{out}}/whitepaper.pdf
    typst compile --root . research/reports/2026-04-21-groth16-baseline.typ \
                                                         {{out}}/2026-04-21-groth16-baseline.pdf
    typst compile --root . research/reports/2026-04-21-zkmcu-first-session.typ \
                                                         {{out}}/2026-04-21-zkmcu-first-session.pdf
    typst compile --root . research/reports/2026-04-22-bls12-381-prediction.typ \
                                                         {{out}}/2026-04-22-bls12-381-prediction.pdf
    typst compile --root . research/reports/2026-04-22-bls12-381-results.typ \
                                                         {{out}}/2026-04-22-bls12-381-results.pdf
    typst compile --root . research/reports/2026-04-22-semaphore-baseline.typ \
                                                         {{out}}/2026-04-22-semaphore-baseline.pdf
    typst compile --root . research/reports/2026-04-23-stark-prediction.typ \
                                                         {{out}}/2026-04-23-stark-prediction.pdf
    typst compile --root . research/reports/2026-04-23-stark-results.typ \
                                                         {{out}}/2026-04-23-stark-results.pdf
    typst compile --root . research/reports/2026-04-24-stark-quadratic-prediction.typ \
                                                         {{out}}/2026-04-24-stark-quadratic-prediction.pdf
    typst compile --root . research/reports/2026-04-24-stark-quadratic-results.typ \
                                                         {{out}}/2026-04-24-stark-quadratic-results.pdf
    typst compile --root . research/reports/2026-04-24-stark-variance-isolation.typ \
                                                         {{out}}/2026-04-24-stark-variance-isolation.pdf
    typst compile --root . research/reports/2026-04-24-stark-bump-alloc.typ \
                                                         {{out}}/2026-04-24-stark-bump-alloc.pdf
    typst compile --root . research/reports/2026-04-24-stark-allocator-matrix.typ \
                                                         {{out}}/2026-04-24-stark-allocator-matrix.pdf
    typst compile --root . research/reports/2026-04-23-umaal-sram-groth16.typ \
                                                         {{out}}/2026-04-23-umaal-sram-groth16.pdf
    typst compile --root . research/reports/2026-04-25-babybear-quartic-cross-isa.typ \
                                                         {{out}}/2026-04-25-babybear-quartic-cross-isa.pdf
    typst compile --root . research/reports/2026-04-24-phase-3-3-revisited.typ \
                                                         {{out}}/2026-04-24-phase-3-3-revisited.pdf

# Rebuild a single doc on change. `just docs-watch research/reports/…`.
docs-watch path:
    typst watch --root . {{path}}

# ---- Web ----------------------------------------------------------------

web-dev:
    cd web && npm run dev

web-build:
    cd web && npm run build
