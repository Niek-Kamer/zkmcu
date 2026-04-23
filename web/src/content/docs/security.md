---
title: Security
description: What zkmcu defends against, what it assumes, and what is not yet validated.
---

## Threat model

An attacker controls the proof bytes, the public-input bytes, and sometimes the verifying-key bytes. Their goals, in order of severity:

1. **Forgery**, get `verify` to return `Ok(true)` for a proof they didn't legitimately generate
2. **Denial of service**, make the verifier panic, hang, OOM, or reset the host device
3. **Malleability**, find two different encodings of the same logical input to break identity-based invariants (nullifiers, replay tags, Merkle leaves)

zkmcu targets all three. The threat model is shared across all three verifier crates, `zkmcu-verifier` (BN254 Groth16), `zkmcu-verifier-bls12` (BLS12-381 Groth16), and `zkmcu-verifier-stark` (winterfell STARK). Same parser shape, same DoS-hardening, same strict canonical-encoding checks where they apply.

Proof-system-specific notes:

- **BN254**: enforces strict `Fr < r` canonical encoding (stricter than `substrate-bn`'s default which silently reduces mod `r`). Matters for nullifier-style applications.
- **BLS12-381**: enforces that the 16-byte leading-zero padding on every `Fp` element is exact-zero, rejecting any non-zero bits as `Error::InvalidFp`. Without that check an attacker could flip padding bits and the proof would still decode to the same curve point, a trivial malleability vector closed at parse time.
- **STARK**: enforces `MinConjecturedSecurity(95)` at the verifier level, a prover submitting a proof with weaker options is rejected even if the underlying crypto verifies. This prevents downgrade attacks where an attacker submits a 63-bit-secure proof in place of a 95-bit one.

## What's tested

### Adversarial unit tests

23 tests under [`zkmcu-verifier/tests/adversarial.rs`](https://github.com/Niek-Kamer/zkmcu/blob/main/crates/zkmcu-verifier/tests/adversarial.rs) covering:

- Empty, truncated, and oversized inputs to every parser
- Field elements ≥ their respective moduli
- Points not on the curve (e.g. `G1::(1, 1)`, which fails `y² = x³ + 3`)
- Adversarial `num_ic` / `count` fields including `u32::MAX`
- All-zero inputs (identity points, must parse, must not spuriously verify)
- Cross-vector mismatches (square proof against squares-5 VK, etc.)
- **Exhaustive single-bit flip** of every byte in a known-good VK, proof, and public-inputs buffer, zero mutations produce `Ok(true)`

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

`parse_vk` and `parse_public` previously called `Vec::with_capacity(n)` where `n` came from untrusted input. An attacker sending `num_ic = u32::MAX` triggered a `u32::MAX × 96 B ≈ 412 GB` allocation, SIGABRT on desktop, instant reset on MCU.

Patched in v0.1.0 with checked arithmetic plus buffer-length validation before allocation. Verified by the `parse_vk_claimed_ic_count_overflows` and `parse_public_count_astronomical` tests.

### `Fr` non-canonical encoding accepted

`substrate-bn::Fr::from_slice` silently reduces 256-bit inputs mod `r` instead of rejecting non-canonical encodings. Pairing correctness is unaffected (reduction preserves the pairing result), but the behaviour introduces malleability for any application that uses the raw `Fr` bytes as an identity, nullifiers, replay-protection tags, Merkle leaves.

Patched in v0.1.0 with a strict `< r` check in `read_fr_at` before delegating to `substrate-bn`.

## Timing properties

Remote-timing-oracle resistance is a property zkmcu actively measures, not one it tries to formally prove. See [Deterministic timing](/determinism/) for the full methodology. The short version:

| Verifier | Std-dev variance (M33) | Side-channel posture |
|---|---:|---|
| BN254 Groth16 | ~0.05 % | Low allocator activity, naturally tight |
| BLS12-381 Groth16 | ~0.05 % | Same as BN254 |
| **STARK (TlsfHeap)** | **0.08 %** | Deterministic allocator brings variance to silicon floor |
| STARK (LlffHeap) | ~0.25 % | Allocator noise obscures crypto timing |

Under the recommended `TlsfHeap` allocator config, all three verifiers produce sub-0.1 % iteration-to-iteration variance. That's below the noise floor of any non-lab-grade timing oracle (USB / BLE / network transports have millisecond-or-worse resolution). *This is not a claim of formal constant-time execution*, it's a claim that the observable timing channel in a realistic deployment is indistinguishable from silicon noise.

For applications where secret data flows into the verify code path (not the usual zkmcu use case), the picture changes, `substrate-bn` and `bls12_381` both have scalar-dependent code paths that a lab-grade attacker with full-cycle-precision measurement could exploit. zkmcu's threat model doesn't cover that case.

## Explicit out-of-scope for v0.1.0

Documented as not-yet-validated, not quietly skipped:

- **Lab-grade constant-time execution.** As above: observable-to-remote-attacker timing is in the noise floor, but full-cycle CT would require a whole-verifier audit across winterfell's internal code paths (for STARK) and `substrate-bn` / `bls12_381` (for Groth16). Acceptable for verify-only threat models where the proof and public inputs are already public. **Not acceptable if secret data ever flows into the verify code path.**
- **Power analysis / EM leakage.** Unmeasured. Requires a ChipWhisperer-class lab setup. Treated as a separate follow-up project.
- **G2 subgroup membership (Groth16).** Whether `substrate-bn`'s and `bls12_381`'s pairing routines reject points on the G2 twist that are not in the prime-order subgroup is trusted from the upstream libraries and untested by zkmcu directly. Historically a bug class in BN254 precompile implementations. On the audit list.
- **STARK security conjecture.** 95-bit "conjectured" security relies on the list-decoding bound assumed by most deployed STARK systems. Provable security is lower by a factor of 2 queries. Acceptable in practice, but worth documenting as "conjectured" not "proven".
- **Trusted VK assumption (Groth16).** An adversary who controls the VK can in principle engineer the pairing check to accept a forged proof. zkmcu assumes the VK is trusted, baked into firmware at provisioning time, or loaded from a trusted channel. If your use case loads the VK dynamically from an untrusted source, that is a separate threat model and zkmcu does not defend against it.
- **Trusted AIR assumption (STARK).** Analogous to the Groth16 VK assumption, the AIR definition compiled into the verifier binary is the integrity anchor. An adversary who changes the AIR source before compilation can build a verifier that accepts proofs it shouldn't. STARK upgrades therefore require signed firmware updates, not runtime configuration.

## Reporting a vulnerability

Open a [GitHub security advisory](https://github.com/Niek-Kamer/zkmcu/security/advisories) with reproduction steps and the affected `zkmcu-verifier` version. Default disclosure window is 90 days from first report, earlier if a patch ships sooner.
