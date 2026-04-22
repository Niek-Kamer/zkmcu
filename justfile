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
lint: lint-host lint-m33 lint-m33-bls12 lint-rv32 lint-rv32-bls12

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

# Everything that must pass before a commit.
check: fmt-check lint test

# Full gate including every firmware build, used before cutting a benchmark run.
check-full: check build-m33 build-m33-bls12 build-rv32 build-rv32-bls12

# Regenerate the committed test vectors.
regen-vectors:
    cargo run -p zkmcu-host-gen --release

# Report outdated dependencies. Requires `cargo install cargo-outdated` once.
outdated:
    cargo outdated --workspace --root-deps-only

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

# Rebuild a single doc on change. `just docs-watch research/reports/…`.
docs-watch path:
    typst watch --root . {{path}}

# ---- Web ----------------------------------------------------------------

web-dev:
    cd web && npm run dev

web-build:
    cd web && npm run build
