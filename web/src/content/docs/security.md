---
title: Security
description: What zkmcu defends against, what it assumes, and what is not yet validated.
---

## Threat model

An attacker controls the proof bytes, the public-input bytes, and sometimes the verifying-key bytes. Their goals, in order of severity:

1. **Forgery** — get `verify` to return `Ok(true)` for a proof they didn't legitimately generate
2. **Denial of service** — make the verifier panic, hang, OOM, or reset the host device
3. **Malleability** — find two different encodings of the same logical input to break identity-based invariants (nullifiers, replay tags, Merkle leaves)

zkmcu targets all three. The threat model is identical for both the BN254 (`zkmcu-verifier`) and BLS12-381 (`zkmcu-verifier-bls12`) verifier crates — same parser shape, same DoS-hardening, same strict canonical-encoding checks. EIP-2537 adds one additional check not present in EIP-197: the 16-byte leading-zero padding on every `Fp` element is verified exact-zero, rejecting any non-zero bits there as `Error::InvalidFp`. Without that check an attacker could flip padding bits and the proof would still decode to the same curve point — a trivial malleability vector closed at parse time.

## What's tested

### Adversarial unit tests

23 tests under [`zkmcu-verifier/tests/adversarial.rs`](https://github.com/Niek-Kamer/zkmcu/blob/main/crates/zkmcu-verifier/tests/adversarial.rs) covering:

- Empty, truncated, and oversized inputs to every parser
- Field elements ≥ their respective moduli
- Points not on the curve (e.g. `G1::(1, 1)`, which fails `y² = x³ + 3`)
- Adversarial `num_ic` / `count` fields including `u32::MAX`
- All-zero inputs (identity points — must parse, must not spuriously verify)
- Cross-vector mismatches (square proof against squares-5 VK, etc.)
- **Exhaustive single-bit flip** of every byte in a known-good VK, proof, and public-inputs buffer — zero mutations produce `Ok(true)`

### Property-based tests

6 properties under [`zkmcu-verifier/tests/properties.rs`](https://github.com/Niek-Kamer/zkmcu/blob/main/crates/zkmcu-verifier/tests/properties.rs), each running 256 generated cases per invocation via `proptest`:

- No random byte sequence up to 4 KB panics any of the three parsers
- If all three parsers succeed, `verify` never panics
- Random XOR masks applied to proof bytes never produce `Ok(true)`
- Random XOR masks applied to public-input bytes never produce `Ok(true)`

### Cross-library consistency

Every committed test vector is produced by `arkworks` and natively verified there before being written. The embedded path re-verifies the exact same bytes with `substrate-bn`. If either library drifts from EIP-197, the test breaks. See [architecture](/architecture/#cross-library-consistency).

## Known findings (fixed)

### DoS via unbounded allocation

`parse_vk` and `parse_public` previously called `Vec::with_capacity(n)` where `n` came from untrusted input. An attacker sending `num_ic = u32::MAX` triggered a `u32::MAX × 96 B ≈ 412 GB` allocation — SIGABRT on desktop, instant reset on MCU.

Patched in v0.1.0 with checked arithmetic plus buffer-length validation before allocation. Verified by the `parse_vk_claimed_ic_count_overflows` and `parse_public_count_astronomical` tests.

### `Fr` non-canonical encoding accepted

`substrate-bn::Fr::from_slice` silently reduces 256-bit inputs mod `r` instead of rejecting non-canonical encodings. Pairing correctness is unaffected (reduction preserves the pairing result), but the behaviour introduces malleability for any application that uses the raw `Fr` bytes as an identity — nullifiers, replay-protection tags, Merkle leaves.

Patched in v0.1.0 with a strict `< r` check in `read_fr_at` before delegating to `substrate-bn`.

## Explicit out-of-scope for v0.1.0

Documented as not-yet-validated, not quietly skipped:

- **Constant-time execution.** `substrate-bn` is not constant-time. Verify duration varies observably with public-input Hamming weight (see the [scaling benchmark](/benchmarks/)). Acceptable for verify-only threat models where the proof and public inputs are already public. **Not acceptable if secret data ever flows into the verify code path.**
- **Power analysis / EM leakage.** Unmeasured. Requires a ChipWhisperer-class lab setup. Treated as a separate follow-up project.
- **G2 subgroup membership.** Whether `substrate-bn`'s pairing routine rejects points on the G2 twist that are not in the prime-order subgroup is trusted from the upstream library and untested by zkmcu directly. Historically a bug class in BN254 precompile implementations. On the audit list.
- **Trusted VK assumption.** An adversary who controls the VK can in principle engineer the pairing check to accept a forged proof. zkmcu assumes the VK is trusted — baked into firmware at provisioning time, or loaded from a trusted channel. If your use case loads the VK dynamically from an untrusted source, that is a separate threat model and zkmcu does not defend against it.

## Reporting a vulnerability

Open a [GitHub security advisory](https://github.com/Niek-Kamer/zkmcu/security/advisories) with reproduction steps and the affected `zkmcu-verifier` version. Default disclosure window is 90 days from first report — earlier if a patch ships sooner.
