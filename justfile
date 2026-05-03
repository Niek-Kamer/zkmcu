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

# Build the STARK Fibonacci prover firmware for the Pico 2 W (Cortex-M33).
build-m33-stark-prover:
    cd crates/bench-rp2350-m33-stark-prover && cargo build --release

# Build the BabyBear+Quartic STARK prover firmware for the Pico 2 W (Cortex-M33).
build-m33-stark-prover-bb:
    cd crates/bench-rp2350-m33-stark-prover-bb && cargo build --release

# Build the BN254 ASM selftest firmware for the Pico 2 W (Cortex-M33).
build-m33-bn-asm-test:
    cd crates/bench-rp2350-m33-bn-asm-test && cargo build --release

# Build the timing oracle firmware for the Pico 2 W (Cortex-M33).
build-m33-timing-oracle:
    cd crates/bench-rp2350-m33-timing-oracle && cargo build --release

# Build the STARK Fibonacci prover firmware for the Pico 2 W (Hazard3 RV32).
build-rv32-stark-prover:
    cd crates/bench-rp2350-rv32-stark-prover && cargo build --release

# Build the BabyBear+Quartic STARK prover firmware for the Pico 2 W (Hazard3 RV32).
build-rv32-stark-prover-bb:
    cd crates/bench-rp2350-rv32-stark-prover-bb && cargo build --release

# Build the firmware for the Pico 2 W (Hazard3 RV32, STARK Fibonacci, Goldilocks baseline).
build-rv32-stark:
    cd crates/bench-rp2350-rv32-stark && cargo build --release

# Build the Hazard3 RV32 STARK firmware with the BabyBear + Quartic fork (phase 3.3).
build-rv32-stark-bb:
    cd crates/bench-rp2350-rv32-stark && cargo build --release --features babybear

# Build the Plonky3 PQ-Poseidon-chain verifier firmware for the Pico 2 W (Cortex-M33, phase 4.0).
build-m33-pq-poseidon-chain:
    cd crates/bench-rp2350-m33-pq-poseidon-chain && cargo build --release

# Build the Plonky3 PQ-Poseidon-chain verifier firmware for the Pico 2 W (Hazard3 RV32, phase 4.0).
build-rv32-pq-poseidon-chain:
    cd crates/bench-rp2350-rv32-pq-poseidon-chain && cargo build --release

# Build the Plonky3 PQ-Semaphore verifier firmware for the Pico 2 W (Cortex-M33, phase 4.0 headline).
build-m33-pq-semaphore:
    cd crates/bench-rp2350-m33-pq-semaphore && cargo build --release

# Build the Plonky3 PQ-Semaphore verifier firmware for the Pico 2 W (Hazard3 RV32, phase 4.0 headline).
build-rv32-pq-semaphore:
    cd crates/bench-rp2350-rv32-pq-semaphore && cargo build --release

# Build the PQ-Semaphore reject-time benchmark for the Pico 2 W (Cortex-M33, phase C two-stage early exit).
build-m33-pq-semaphore-reject:
    cd crates/bench-rp2350-m33-pq-semaphore-reject && cargo build --release

# Build the PQ-Semaphore reject-time benchmark for the Pico 2 W (Hazard3 RV32, phase C two-stage early exit).
build-rv32-pq-semaphore-reject:
    cd crates/bench-rp2350-rv32-pq-semaphore-reject && cargo build --release

# Build the PQ-Semaphore Goldilocks-Quadratic benchmark for the Pico 2 W (Cortex-M33, phase D parallel track).
build-m33-pq-semaphore-gl:
    cd crates/bench-rp2350-m33-pq-semaphore-gl && cargo build --release

# Build the PQ-Semaphore Goldilocks-Quadratic benchmark for the Pico 2 W (Hazard3 RV32, phase D parallel track).
build-rv32-pq-semaphore-gl:
    cd crates/bench-rp2350-rv32-pq-semaphore-gl && cargo build --release

# Build the PQ-Semaphore stacked dual-hash (Poseidon2 + Blake3) benchmark for the Pico 2 W (Cortex-M33, phase E.1).
build-m33-pq-semaphore-dual:
    cd crates/bench-rp2350-m33-pq-semaphore-dual && cargo build --release

# Build the PQ-Semaphore stacked dual-hash (Poseidon2 + Blake3) benchmark for the Pico 2 W (Hazard3 RV32, phase E.1).
build-rv32-pq-semaphore-dual:
    cd crates/bench-rp2350-rv32-pq-semaphore-dual && cargo build --release

# Run every native test (cross-check: arkworks <-> substrate-bn).
test:
    cargo test --release

# Run vendor/bn unit tests (field arithmetic, Fq2/Fq6/Fq12, pairings).
test-bn:
    cd vendor/bn && cargo test --release

# Fq2 micro-benchmark. Run before and after patching to compare ns/op.
bench-fq2:
    cd vendor/bn && cargo test --release -- bench_fq2_ops --nocapture

# Check formatting (does not modify files). The 2>&1 + grep filter
# suppresses rustfmt's "unstable features are only available in nightly"
# warnings, wich come from vendored crates' rustfmt.toml files setting
# options like `imports_granularity = "Module"` and propagate per
# rustfmt invocation. The warnings are cosmetic (rustfmt ignores those
# options on stable and still formats correctly), but they spam ~60
# lines of output per fmt-check call. Filtering keeps real diff output
# (`Diff in path:line` lines) unchanged so format violations still
# fail the recipe.
fmt-check:
    #!/usr/bin/env bash
    out=$(cargo fmt --all --check 2>&1)
    # Strip the unstable-feature warnings (vendored rustfmt configs use
    # nightly-only options).
    out=$(echo "$out" | grep -vE 'imports_granularity|group_imports|unstable_features' || true)
    # Strip vendor/ diff blocks. cargo fmt --all walks into vendored
    # crates via path deps even though they're in workspace.exclude;
    # those crates have their own rustfmt configs (with unstable opts)
    # and aren't ours to enforce. Each diff block is "Diff in path:line:"
    # followed by the diff body, terminated at the next "Diff in" header
    # or end of output.
    out=$(echo "$out" | awk '
        /^Diff in / {
            in_vendor = ($0 ~ /\/vendor\//)
            if (in_vendor) next
            print
            next
        }
        { if (!in_vendor) print }
    ')
    # If any "Diff in" header survived, real workspace format issue, fail.
    if echo "$out" | grep -q '^Diff in'; then
        echo "$out"
        exit 1
    fi
    exit 0

# Format every crate in the workspace.
fmt:
    cargo fmt --all

# Clippy at -D warnings. Host crates first (default-members), then each firmware
# crate separately against its own target.
lint: lint-host lint-m33 lint-m33-bls12 lint-m33-stark lint-m33-stark-bb lint-m33-stark-prover lint-m33-stark-prover-bb lint-m33-pq-poseidon-chain lint-m33-pq-semaphore lint-m33-pq-semaphore-reject lint-m33-pq-semaphore-gl lint-m33-pq-semaphore-dual lint-m33-bn-asm-test lint-m33-timing-oracle lint-rv32 lint-rv32-bls12 lint-rv32-stark lint-rv32-stark-bb lint-rv32-stark-prover lint-rv32-stark-prover-bb lint-rv32-pq-poseidon-chain lint-rv32-pq-semaphore lint-rv32-pq-semaphore-reject lint-rv32-pq-semaphore-gl lint-rv32-pq-semaphore-dual

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

lint-m33-stark-prover:
    cd crates/bench-rp2350-m33-stark-prover && cargo clippy --release -- -D warnings

lint-m33-stark-prover-bb:
    cd crates/bench-rp2350-m33-stark-prover-bb && cargo clippy --release -- -D warnings

lint-m33-bn-asm-test:
    cd crates/bench-rp2350-m33-bn-asm-test && cargo clippy --release -- -D warnings

lint-m33-timing-oracle:
    cd crates/bench-rp2350-m33-timing-oracle && cargo clippy --release -- -D warnings

lint-m33-stark-bb:
    cd crates/bench-rp2350-m33-stark && cargo clippy --release --features babybear -- -D warnings

lint-rv32-stark:
    cd crates/bench-rp2350-rv32-stark && cargo clippy --release -- -D warnings

lint-rv32-stark-bb:
    cd crates/bench-rp2350-rv32-stark && cargo clippy --release --features babybear -- -D warnings

lint-rv32-stark-prover:
    cd crates/bench-rp2350-rv32-stark-prover && cargo clippy --release -- -D warnings

lint-rv32-stark-prover-bb:
    cd crates/bench-rp2350-rv32-stark-prover-bb && cargo clippy --release -- -D warnings

lint-m33-pq-poseidon-chain:
    cd crates/bench-rp2350-m33-pq-poseidon-chain && cargo clippy --release -- -D warnings

lint-rv32-pq-poseidon-chain:
    cd crates/bench-rp2350-rv32-pq-poseidon-chain && cargo clippy --release -- -D warnings

lint-m33-pq-semaphore:
    cd crates/bench-rp2350-m33-pq-semaphore && cargo clippy --release -- -D warnings

lint-rv32-pq-semaphore:
    cd crates/bench-rp2350-rv32-pq-semaphore && cargo clippy --release -- -D warnings

lint-m33-pq-semaphore-reject:
    cd crates/bench-rp2350-m33-pq-semaphore-reject && cargo clippy --release -- -D warnings

lint-rv32-pq-semaphore-reject:
    cd crates/bench-rp2350-rv32-pq-semaphore-reject && cargo clippy --release -- -D warnings

lint-m33-pq-semaphore-gl:
    cd crates/bench-rp2350-m33-pq-semaphore-gl && cargo clippy --release -- -D warnings

lint-rv32-pq-semaphore-gl:
    cd crates/bench-rp2350-rv32-pq-semaphore-gl && cargo clippy --release -- -D warnings

lint-m33-pq-semaphore-dual:
    cd crates/bench-rp2350-m33-pq-semaphore-dual && cargo clippy --release -- -D warnings

lint-rv32-pq-semaphore-dual:
    cd crates/bench-rp2350-rv32-pq-semaphore-dual && cargo clippy --release -- -D warnings

# Everything that must pass before a commit.
check: fmt-check lint test

# Full gate including every firmware build, used before cutting a benchmark run.
check-full: check build-m33 build-m33-bls12 build-m33-stark build-m33-stark-bb build-rv32 build-rv32-bls12 build-rv32-stark build-rv32-stark-bb

# ---- CI mirror ----------------------------------------------------------
#
# The four `ci-*` recipes below are the SINGLE SOURCE OF TRUTH for what
# `.github/workflows/ci.yml` runs. CI calls `just ci-<job>` directly.
# Lefthook pre-push runs `just ci-all`. Local + CI cannot drift: editing
# any recipe here updates both at once.

# CI host job: fmt + host clippy + test, exactly what the `host` job runs.
ci-host: fmt-check
    cargo clippy -p zkmcu-verifier -p zkmcu-verifier-bls12 -p zkmcu-verifier-stark -p zkmcu-vectors -p zkmcu-host-gen -p zkmcu-bump-alloc -p zkmcu-poseidon-circuit -p zkmcu-poseidon-audit -p zkmcu-babybear --all-targets --release -- -D warnings
    cargo test --release

# CI Cortex-M33 job: cross-compile checks + clippy + build for every m33 firmware crate.
ci-firmware-m33:
    cargo check -p zkmcu-verifier-bls12 --release --target thumbv8m.main-none-eabihf
    cargo check -p zkmcu-verifier-stark --release --target thumbv8m.main-none-eabihf
    cargo check -p zkmcu-verifier-plonky3 --release --target thumbv8m.main-none-eabihf
    just lint-m33 build-m33
    just lint-m33-bls12 build-m33-bls12
    just lint-m33-stark build-m33-stark
    just lint-m33-stark-bb build-m33-stark-bb
    just lint-m33-stark-prover build-m33-stark-prover
    just lint-m33-stark-prover-bb build-m33-stark-prover-bb
    just lint-m33-pq-poseidon-chain build-m33-pq-poseidon-chain
    just lint-m33-pq-semaphore build-m33-pq-semaphore
    just lint-m33-bn-asm-test build-m33-bn-asm-test
    just lint-m33-timing-oracle build-m33-timing-oracle

# CI Hazard3 RV32 job: same shape as the m33 job, riscv32imac target.
ci-firmware-rv32:
    cargo check -p zkmcu-verifier-bls12 --release --target riscv32imac-unknown-none-elf
    cargo check -p zkmcu-verifier-stark --release --target riscv32imac-unknown-none-elf
    cargo check -p zkmcu-verifier-plonky3 --release --target riscv32imac-unknown-none-elf
    just lint-rv32 build-rv32
    just lint-rv32-bls12 build-rv32-bls12
    just lint-rv32-stark build-rv32-stark
    just lint-rv32-stark-bb build-rv32-stark-bb
    just lint-rv32-stark-prover build-rv32-stark-prover
    just lint-rv32-stark-prover-bb build-rv32-stark-prover-bb
    just lint-rv32-pq-poseidon-chain build-rv32-pq-poseidon-chain
    just lint-rv32-pq-semaphore build-rv32-pq-semaphore

# CI docs job: compile every Typst document under research/. Gracefully
# skips with a warning if typst isn't installed locally, so the pre-push
# hook doesn't block contributors who haven't installed it. CI's runner
# always has typst from the workflow setup-typst step.
ci-docs:
    @command -v typst >/dev/null 2>&1 || { echo "typst not installed locally, skipping docs build (CI will run it)"; exit 0; }
    just docs

# Run every CI job locally, in the same order CI runs them.
ci-all: ci-host ci-firmware-m33 ci-firmware-rv32 ci-docs

# Regenerate the committed test vectors.
regen-vectors:
    cargo run -p zkmcu-host-gen --release
    cargo run -p zkmcu-host-gen --release -- poseidon
    cargo run -p zkmcu-host-gen --release -- pq-poseidon-chain
    cargo run -p zkmcu-host-gen --release -- pq-semaphore

# Phase-1 measurement: constraint counts + proving key sizes for Poseidon Merkle.
measure-poseidon:
    cargo run -p zkmcu-host-gen --release -- measure-poseidon

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
    just fuzz pq_semaphore_parse_proof_p2 {{SECS}}
    just fuzz pq_semaphore_parse_proof_b3 {{SECS}}
    just fuzz pq_semaphore_parse_public {{SECS}}
    just fuzz pq_semaphore_dual_parse_and_verify {{SECS}}

# Phase G campaign: longer per-target run on the four PQ-Semaphore parser
# targets. Defaults to 1 hour each (4 hours total). Use a smaller SECS for
# a quick "did anything regress" pass.
fuzz-pq-campaign SECS="3600":
    just fuzz pq_semaphore_parse_proof_p2 {{SECS}}
    just fuzz pq_semaphore_parse_proof_b3 {{SECS}}
    just fuzz pq_semaphore_parse_public {{SECS}}
    just fuzz pq_semaphore_dual_parse_and_verify {{SECS}}

# Regenerate Phase G fuzz seed corpora from the committed dual-leg vectors.
# Idempotent — re-run after `just regen-vectors` bumps `pq-semaphore-d10-dual`.
fuzz-seeds-pq:
    python3 fuzz/scripts/regen_pq_semaphore_seeds.py

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
    typst compile --root . research/reports/2026-04-24-babybear-quartic-cross-isa.typ \
                                                         {{out}}/2026-04-24-babybear-quartic-cross-isa.pdf
    typst compile --root . research/reports/2026-04-24-phase-3-3-revisited.typ \
                                                         {{out}}/2026-04-24-phase-3-3-revisited.pdf
    typst compile --root . research/reports/2026-04-26-stark-prover-results.typ \
                                                         {{out}}/2026-04-26-stark-prover-results.pdf
    typst compile --root . research/reports/2026-04-26-stark-prover-bb-results.typ \
                                                         {{out}}/2026-04-26-stark-prover-bb-results.pdf
    typst compile --root . research/reports/2026-04-29-pq-semaphore-scoping.typ \
                                                         {{out}}/2026-04-29-pq-semaphore-scoping.pdf
    typst compile --root . research/reports/2026-04-29-pq-semaphore-results.typ \
                                                         {{out}}/2026-04-29-pq-semaphore-results.pdf

# Rebuild a single doc on change. `just docs-watch research/reports/…`.
docs-watch path:
    typst watch --root . {{path}}

# ---- Web ----------------------------------------------------------------

web-dev:
    cd web && npm run dev

web-build:
    cd web && npm run build
