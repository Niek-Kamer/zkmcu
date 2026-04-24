# zkmcu fuzzing

`cargo-fuzz` harness for the three verifier crates. Workspace-excluded so
the fuzz build (nightly, own profile, `#![no_main]`) stays out of the
regular `cargo check` path.

## Targets

| target                     | covers                                               |
|----------------------------|------------------------------------------------------|
| `bn254_parse_vk`           | `zkmcu_verifier::parse_vk` on arbitrary bytes        |
| `bn254_parse_proof`        | `zkmcu_verifier::parse_proof` (fixed 256 B wire)     |
| `bn254_parse_public`       | `zkmcu_verifier::parse_public` (count + Fr check)    |
| `bn254_verify_bytes`       | full `verify_bytes`, tri-splits input via length prefixes |
| `bls12_parse_vk`           | `zkmcu_verifier_bls12::parse_vk` (EIP-2537 layout)   |
| `bls12_parse_proof`        | `zkmcu_verifier_bls12::parse_proof` (fixed 512 B)    |
| `bls12_parse_public`       | `zkmcu_verifier_bls12::parse_public`                 |
| `stark_parse_proof`        | `zkmcu_verifier_stark::parse_proof` (winterfell 0.13 wire) |

Every target's invariant is the same: the parser/verifier must return
`Ok` or `Err` cleanly, never panic / abort / hang. Panic on adversarial
input is a firmware DoS surface under `panic-halt` and violates the
`SECURITY.md` threat model.

## One-time setup

`cargo-fuzz` uses `-Zsanitizer=address` and related `-Z` flags wich only
nightly rustc accepts. This directory pins nightly via
[`rust-toolchain.toml`](./rust-toolchain.toml), so rustup auto-selects
the right toolchain, but you need nightly installed once:

```sh
rustup toolchain install nightly
cargo install cargo-fuzz
```

The rest of the workspace stays on stable. The pin is scoped to
`fuzz/`.

## Running

```sh
# 60 s on a single target.
just fuzz bn254_parse_vk

# Custom time.
just fuzz bn254_parse_vk 600

# Quick smoke-test, 10 s on each target in sequence.
just fuzz-smoke

# List targets.
just fuzz-list
```

First-run cost: `cargo +nightly fuzz run` downloads + builds the
libFuzzer runtime and instrumented versions of every dep. ~2-3 min for
the first target; subsequent targets reuse artifacts.

## Seeds

`fuzz/seeds/<target>/` holds known-good inputs copied from
`crates/zkmcu-vectors/data/`. libFuzzer starts from these and mutates
outward, better initial coverage than starting from `\0`. The
`corpus/` directory (gitignored) is populated at run time.

To refresh seeds after a fixture regen:

```sh
# From the fuzz/ directory
rm -rf seeds/*/
# Re-run the copy block from the patch that first added this harness,
# or use `cargo fuzz cmin <target>` to minimize the active corpus.
```

## Adding a target

1. Add `[[bin]]` entry to `fuzz/Cargo.toml` with name + path.
2. Write `fuzz_targets/<name>.rs` following the existing pattern:
   - `#![no_main]`
   - `use libfuzzer_sys::fuzz_target;`
   - `fuzz_target!(|data: &[u8]| { let _ = thing_under_test(data); });`
3. Add seed files under `fuzz/seeds/<name>/`.
4. Add a justfile entry to `fuzz-smoke` so it runs by default.

## Known history

This harness landed after proptest (in `crates/zkmcu-verifier-stark/tests/properties.rs`)
found two STARK panic paths. Both were fixed the same day, writeup at
`research/postmortems/2026-04-24-stark-cross-field-panic.typ`. Fuzz then
found two more a day later, closed in the winterfell fork, writeup at
`research/postmortems/2026-04-24-stark-unbounded-vec-alloc.typ`. The fuzz
suite should keep extending that coverage. Any new incident gets a
`research/postmortems/<date>-<slug>.typ` writeup and a fix before the next
release.
