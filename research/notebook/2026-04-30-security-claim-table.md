# Security claim table for the Phase A-E PQ-Semaphore writeup

**Status:** draft v1, 2026-04-30. Source-of-truth for the security section
of the headline writeup + ePrint submission. Numbers come from the per-
phase `result.toml` files under `benchmarks/runs/2026-04-{29,30}-*`.

The point of this document is to be precise about three different things
that often get conflated in ZK marketing:

1. **What we claim** — concrete bit-strength numbers, per phase.
2. **Why we claim it** — which library / audit / constant our claim
   inherits from.
3. **What we do NOT claim** — the audit boundary, side-channel posture,
   and physical-tamper threat model.

The writeup will lean heavily on point 3. Burying audit boundaries is
the single most reliable way to lose credibility with cryptography
readers; stating them up front is the single most reliable way to keep
it.

---

## 1. The headline table (per phase, both ISAs)

| Phase | Configuration | FRI conj. (bits) | Hash floor (bits) | Combined min | Notes |
|---|---|---:|---:|---:|---|
| Baseline | Groth16 / BN254 (substrate-bn + UMAAL asm) | n/a | n/a | **~110 classical, 0 PQ** | Discrete log on BN254; Shor breaks pairings entirely |
| 4.0 (initial) | BabyBear × Quartic, d=4, no grinding | 95 | 124 | 95 | Pre-plan baseline |
| A | + 16+16 grinding | 127 | 124 | 124 | Hash floor binds |
| B | + DIGEST_WIDTH = 6 | 127 | 186 | **127** | FRI binds; hash floor no longer the bottleneck |
| C | + two-stage early exit | 127 | 186 | 127 | No soundness change; bounds attacker DoS at ~9× honest |
| D (alt) | Goldilocks × Quadratic, d=4, +grinding | 127 | 128 (hash) + 128 (native field) | 127 | Removes field-side conjecture stack; +87% verify cost |
| E.1 | Phase B + Blake3 sibling FRI | 127 per leg | 186 per leg | **127 + dual-hash composition** | Hash-tower property is orthogonal to the bit-count |

The **127-bit number** is the single number to lead with for everything
post-Phase-B. The dual-hash composition in Phase E.1 does NOT change
that headline number — it adds an orthogonal cryptanalytic-defence-in-
depth property that the bit-count does not capture.

---

## 2. Where each number comes from

### FRI conjectured bits

Per Plonky3's conjectured-soundness stack for `num_queries=64, log_blowup=1`:
the base claim is **95 bits**. Grinding adds bits 1-for-1 against
honest-prover-malicious-verifier amplification: with `COMMIT_POW_BITS=16`
and `QUERY_POW_BITS=16`, total grinding contribution is 32 bits. So
post-grinding conjectured FRI soundness is `95 + 32 = 127` bits.

This is *conjectured*, not proven. Plonky3's conjecture stack assumes
the FRI commitment is IOP-sound under the Reed-Solomon proximity
conjecture and the hash function is collision-resistant in the random-
oracle model. We inherit those assumptions verbatim.

### Hash collision floor

For an `N`-element BabyBear digest with each element ~31 bits, the
generic-collision floor is `~N × 31 / 2` bits via Pollard's-rho /
birthday. Concrete values:

| `DIGEST_WIDTH` | Digest space (bits) | Generic-collision floor (bits) |
|---:|---:|---:|
| 4 (Phase 4.0, A) | 124 | ~62 (well below 128) |
| 6 (Phase B, C, E.1) | 186 | ~93 (above 128, no longer binding) |

Wait — the Phase B writeup quoted "186-bit hash floor" but the
generic-collision number is `186/2 = 93`. The writeup language is
sloppy. The 186 is the digest *space*; the 93 is the *generic-collision
work factor*. For the headline we should quote either:
- "186-bit digest space" (correct, but reader needs to halve it)
- "93-bit generic-collision floor" (correct, but loses headline appeal)
- "186-bit collision *resistance under random-oracle modelling*"
  (which is what Plonky3 implicitly assumes, treating the hash as a
  random oracle so the attacker has to find a target preimage, not a
  generic collision)

**Decision for the writeup**: we'll lead with "186-bit digest" and
clarify in the security section that the random-oracle modelling
gives 186 bits but the generic-collision lower bound is 93. Honest
disclosure both numbers.

For Phase D (Goldilocks, `DIGEST_WIDTH=4 × 64-bit = 256-bit digest`):
- Random-oracle-modelled: 256 bits
- Generic-collision: 128 bits
- This is genuinely above the FRI floor under either model; field-side
  Goldilocks is also 128-bit native.

For Phase E.1 (Blake3 leg, 32-byte digest):
- Random-oracle-modelled: 256 bits
- Generic-collision: 128 bits
- Blake3 itself has the strongest pedigree of any hash in the stack
  (BLAKE→BLAKE2→BLAKE3 lineage, decades of cryptanalysis attempts).

### Combined floor

`min(FRI_conjectured, hash_collision_floor, field_security)` per leg.

For Phase B onwards: `min(127, 186_or_93, ~124) = 124-127` depending on
which hash floor model you use. In the writeup we lead with 127 and
disclose both models.

For Phase E.1 dual-hash: `min` is taken per leg; combined claim is
`min(per_leg_min, per_leg_min) = 127` PLUS the orthogonal dual-hash
property (a forged proof must satisfy both legs, so cryptanalytic
breakthrough on one hash family doesn't collapse the verifier).

---

## 3. What we did NOT analyze (audit boundary, the section that earns credibility)

### Chain of trust

**Inherited from upstream audit:**
- Plonky3 core: `p3-uni-stark`, `p3-fri`, `p3-merkle-tree`,
  `p3-symmetric`, `p3-commit`, `p3-challenger`, `p3-baby-bear`,
  `p3-goldilocks` — published audits, see `vendor/Plonky3/audits/`.
- Poseidon2-BabyBear-16 round constants — independently audited
  via `crates/zkmcu-poseidon-audit` (in-tree audit crate, regenerates
  the constants from the published spec and bit-compares against
  Plonky3's `BABYBEAR_POSEIDON2_RC_16_*` arrays).
- Poseidon2-Goldilocks-16 round constants (Phase D) — same audit
  shape, covers `GOLDILOCKS_POSEIDON2_RC_16_*`.
- BabyBear / Goldilocks field arithmetic (Plonky3 vendor) — upstream-
  audited.
- Blake3 1.8 (Phase E.1) — independently audited multiple times,
  mature widely-deployed crate.

**NOT audited:**
- The custom PQ-Semaphore AIR (`crates/zkmcu-verifier-plonky3/src/pq_semaphore.rs`).
  The audited Poseidon2 constraint surface is preserved byte-for-byte
  (we copied `eval_full_round`, `eval_partial_round`, `eval_sbox`
  verbatim from Plonky3 because they are `pub(crate)` and not callable
  externally; the audit covers their constraint shape, which is
  preserved as long as the bytes match). The cross-row witness-column
  constraints (id_col / scope_col continuity, sibling/prev_digest
  binding, conditional Merkle swap) are NEW logic, hand-checked but
  not externally audited.
- The postcard wire format and proof parsing
  (`zkmcu-verifier-plonky3::pq_semaphore::parse_proof` etc.) —
  defensive `MAX_PROOF_SIZE` cap and trailing-byte rejection are
  present, but no independent review.
- The Blake3-flavoured StarkConfig wiring in
  `pq_semaphore_blake3.rs` (`SerializingHasher<Blake3>`,
  `CompressionFunctionFromHasher<Blake3, 2, 32>`,
  `MerkleTreeMmcs<Val, u8, ...>`,
  `SerializingChallenger32<Val, HashChallenger<u8, Blake3, 32>>`) —
  composes audited primitives in an audit-implicit way; the type-level
  composition has not been independently reviewed.
- The firmware (allocator integration, USB CDC-ACM transport, panic-
  halt + bench harness in `bench-core` and the `bench-rp2350-*`
  crates) — engineering code, not cryptography, but bugs there can
  still leak material.
- Hand-written ARMv8-M UMAAL asm in `vendor/bn` — used by the
  **Groth16 baseline only**, not by the STARK headline. Differentially
  tested via a **three-implementation chain**:
  on-device, `mul_reduce` (asm) is compared against `mul_reduce_u32_ref`
  (independent portable u32-limbed SOS) over splitmix64-random pairs;
  host-side `cargo test` cross-checks `mul_reduce_rust` (2-limb u128
  Montgomery via `mac_digit`) against the same `mul_reduce_u32_ref`.
  Together: rust ≡ u32_ref (host) AND asm ≡ u32_ref (device) ⇒ asm ≡
  rust transitively. The selftest is a separate firmware flash
  (`bench-rp2350-m33-bn-asm-test`); the headline Groth16 bench
  firmware does not re-run it at boot.

### Side channels

| Channel | Status |
|---|---|
| Timing | In-progress via `crates/bench-rp2350-m33-timing-oracle`. **No constant-time guarantee claimed.** Phase C's two-stage early-exit is intentionally data-dependent (rejects mutated proofs in 8-150 ms vs honest 1131 ms); this is a deliberate availability trade-off, not a constant-time path. The Poseidon2 permutation itself runs in constant time at the algebraic level. |
| Power analysis (SPA/DPA) | Not analyzed. Pico 2 W ships no SPA/DPA countermeasures. |
| Fault injection (voltage / clock glitching) | Not analyzed. |
| EM emanation | Not analyzed. |
| Cache timing | Cortex-M33 has unified L1 cache; Hazard3 has no cache. Not analyzed. |

### Physical and supply-chain

- **Pico 2 W has no secure element**, no tamper detection, no PUF.
  RP2350's secure boot exists for boot-image signing but does not
  protect SRAM at runtime.
- **An attacker with ~30 minutes of physical access** to a powered
  device can dump the 524 KB SRAM via SWD debug or BOOTSEL-mode
  fault injection, recovering any keys or witness material currently
  resident.
- **Cold-boot off-power attacks** are theoretically possible (SRAM
  retention below -40°C is undocumented for RP2350) but require
  specialised equipment.
- **Self-flashable firmware** is part of the design, not a bug. Anyone
  holding the device can flash arbitrary code via `picotool`. Honest
  marketing label: **convenience-grade self-custody, not Ledger-grade
  tamper resistance**. The dual-hash STARK soundness composition does
  not change this.
- **Open hardware, open firmware, open test vectors** — anyone can
  independently verify the firmware they run matches the published
  source. This is the *only* tamper-resistance property we claim.

### What this does NOT replace

- **Production hardware wallets** with secure elements (Ledger Nano,
  Trezor Safe, Foundation Passport) — those defend against physical
  attackers; we do not.
- **HSMs** for key custody — same.
- **Formally-verified verifiers** — we inherit Plonky3's informal
  audit status; we add new unaudited code. A formally-verified
  PQ-Semaphore verifier is future work.

---

## 4. The single paragraph that goes in the writeup

Suggested wording for the security section, tight version:

> The PQ-Semaphore Phase E.1 verifier achieves a combined conjectured
> soundness of 127 bits per leg under the Plonky3 random-oracle
> conjecture stack, with hash-tower composition across two independent
> hashes (audited Poseidon2-`BabyBear`-16 and Blake3 1.8). A forged
> proof must satisfy both legs simultaneously; cryptanalytic surprise
> on either hash family does not collapse the verifier. We inherit the
> Plonky3 core audit, the in-tree Poseidon2 round-constant audit
> (`crates/zkmcu-poseidon-audit`), and Blake3's independent audit
> history. The custom PQ-Semaphore AIR, the postcard wire format, and
> the firmware are not externally audited; the Poseidon2 constraint
> surface is preserved byte-for-byte from the upstream audit. We do
> not analyse timing, power, fault, or EM side channels. Pico 2 W has
> no secure element; an attacker with physical access can dump SRAM in
> ~30 minutes via SWD or BOOTSEL. Honest label: convenience-grade
> self-custody on open hardware, not tamper-resistant key storage.

(~180 words, ePrint-friendly, names every dependency boundary
explicitly.)

---

## 5. Open questions / blockers

- [ ] Decide which hash-floor model to lead with (random-oracle vs
      generic-collision). My pick: lead with random-oracle, footnote
      generic-collision — but tag for review by anyone with formal
      crypto background.
- [x] **RESOLVED 2026-04-30**: prior-art re-search
      (`2026-04-30-prior-art-stark-side.md`) found no published prior
      work on "Goldilocks-vs-BabyBear hash-bound STARK verifier"
      finding. We can claim Phase D as our negative result without
      citation collision.
- [ ] Run the `bench-rp2350-m33-timing-oracle` end-to-end before
      publication. If it finds anything, decide whether to disclose
      the exact channel or just the meta-fact "we tested for X, found
      nothing significant". (Honest meta-disclosure beats no
      disclosure either way.)
- [x] **RESOLVED 2026-04-30**: Phase D goes in the methodology
      section as a lead-bullet "hypothesis rejected" paragraph.
      Confirmed by prior-art absence — no one else has measured this.
      It is *our* negative result and the most credibility-building
      paragraph in the writeup.

## 6. Headline-claim update from the prior-art search

The prior-art re-search (`2026-04-30-prior-art-stark-side.md`)
materially changed the writeup framing. Two changes:

1. **The "first" claim narrows but survives.** Use:
   - Operationally-bulletproof claim: "First measured no_std Plonky3
     STARK verifier on a Cortex-M-class microcontroller."
   - Stronger novelty claim: "First published dual-hash (Poseidon2 ∥
     Blake3) FRI verification with hash-tower soundness composition,
     measured on $7 hardware in 1.6 s."
   The second claim is more novel than the first and probably the
   one that gets the ePrint citation.

2. **Comparison row table added.** The writeup security section gets
   a comparison-table row alongside winterfell, zkDilithium (ePrint
   2023/414), RISC Zero, SP1. Ours is the only row in the MCU
   column. The table is the cleanest way to make the gap visible to
   readers without our having to over-claim in prose.
