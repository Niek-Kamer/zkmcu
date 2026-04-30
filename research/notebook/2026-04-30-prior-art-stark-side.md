# Prior-art re-search — STARK side

**Date:** 2026-04-30
**Scope:** the *STARK* side of the project's prior-art claim. The
SNARK side is already covered in `research/prior-art/main.typ` (Apr 2026
survey for `no_std` Rust Groth16 on Cortex-M).

**Verdict:** the "first" claim survives. Narrow it and lead with two
claims, not one.

---

## Narrowest defensible headline

> **"First measured no_std Rust Plonky3 STARK verifier on a Cortex-M-
> class microcontroller, with cross-ISA (Cortex-M33 + Hazard3 RV32)
> benchmarks, a custom PQ-Semaphore-shaped AIR, and dual-hash
> (Poseidon2 ∥ Blake3) FRI composition."**

Use two claims in the writeup, ordered:

1. **Operationally bulletproof (narrow)**: "First measured no_std
   Plonky3 STARK verifier on a Cortex-M-class microcontroller."
2. **Stronger research contribution (broader)**: "First published
   dual-hash (Poseidon2 ∥ Blake3) FRI verification with hash-tower
   soundness composition, measured on $7 hardware in 1.6 s."

Do **not** claim "first STARK verifier on MCU" without "Plonky3" —
winterfell's `no_std` feature is technically broader prior art even
though no MCU bench was ever published with it.

---

## What the search covered

- ePrint (eprint.iacr.org/search): "STARK Cortex-M", "Plonky3 embedded",
  "no_std STARK", "STARK microcontroller", "FRI Cortex-M", "FRI
  embedded", "post-quantum Semaphore", "PQ Semaphore", "embedded zero-
  knowledge", "STARK verifier hardware", "MCU zero-knowledge".
- arxiv cs.CR same keywords.
- CHES / TCHES proceedings 2022-2025 TOCs.
- GitHub topic search: `stark-verifier`, `plonky3`, `no_std stark`,
  `embedded zk`.
- Project docs and benchmarks pages: RISC Zero, Succinct (SP1),
  Polygon Zero (Plonky3 upstream), Aztec, Powdr, winterfell, Halo 2,
  Nova/SuperNova.
- Plonky3 upstream Discussions and Issues for any embedded-target
  conversation or preliminary numbers.

**Zero hits** for any STARK verifier benchmarked on Cortex-M-class
hardware. All zkVM projects publish verifier numbers only on
server/phone hardware. The gap is real.

---

## Closest five hits (with the gap to our work)

### 1. Plonky3 upstream
- Polygon Zero / 0xPolygon, ongoing.
- README explicitly notes the toolkit relies on x86 BMI1/2, AVX2,
  AVX-512 for performance.
- **Gap**: no MCU target, no embedded port, no published numbers
  outside server/laptop.
- **Use**: the framework whose AVX-targeted optimisation profile makes
  the MCU port itself a contribution. Cite as the platform we extend.

### 2. winterfell (Meta)
- Production STARK prover/verifier in Rust with a `no_std` feature.
- This repo already includes a winterfell verifier in
  `crates/zkmcu-verifier-stark` (Phase 3.x BabyBear × Quartic work).
- **Gap**: `no_std` feature exists but no MCU benchmark has been
  published anywhere.
- **Use**: closest "could-run-on-MCU but unmeasured" baseline. We
  could double-bench against winterfell as an MCU-side comparison
  row, since we already have winterfell wired up. Strong move for
  the writeup if we have the time.

### 3. zkDilithium (ePrint 2023/414, ACSAC 2023)
- Policharla et al., "Post-Quantum Privacy Pass via Post-Quantum
  Anonymous Credentials".
- PQ anonymous-credential scheme using winterfell as STARK backend
  to prove zkDilithium signatures.
- Implementation: <https://github.com/guruvamsi-policharla/zkdilithium>.
- **Gap**: server-class implementation. Proves zkDilithium signatures,
  not Semaphore-shaped Merkle membership. No embedded benchmarks.
- **Use**: closest PQ-anonymous-credential-via-STARK in the
  literature. Important comparison row. They benchmark prover +
  verifier in the same paper; we benchmark verifier only. Their
  verifier on server is `O(seconds)` so the MCU comparison stretches
  *only* in the platform direction, not the time direction — that is
  in fact the point of our work.

### 4. RISC Zero zkVM benchmarks + Succinct SP1 benchmarks
- <https://benchmarks.risczero.com/main/datasheet>,
  <https://blog.succinct.xyz/sp1-benchmarks-8-6-24/>.
- Both publish proving + verification numbers, but only on
  `AWS r6a.16xlarge` (64 vCPU / 512 GB) or M-series Macs.
- Both are zkVMs (general computation) where ours is a custom AIR.
- **Gap**: server-class measurements only; no MCU; zkVM not custom
  AIR; their security analysis is verifier-on-server-CPU not on
  microcontroller.
- **Use**: anchor rows in the "all measured STARK verifiers in
  literature" table. Our row is the only one in the MCU column.

### 5. pqm4 / Falcon-on-Cortex-M4 (ePrint 2025/123 and the wider pqm4 tradition)
- Long-running thread of post-quantum signature schemes on Cortex-M4.
- <https://github.com/mupq/pqm4>.
- **Gap**: signature schemes (sign + verify), not zero-knowledge proofs.
- **Use**: calibration point. Cite as evidence that "PQ verify on
  Cortex-M" is a recognised research thread our work extends from
  signatures to ZK.

---

## Notable absences (the strongest "no prior art" signals)

- **Zero** hits for "Plonky3 microcontroller" / "Plonky3 Cortex-M" /
  "Plonky3 embedded" anywhere — not a single GitHub repo, not a
  single forum post, not a single paper.
- **Zero** CHES/TCHES papers 2022-2025 on STARK or FRI verification
  on embedded hardware.
- GitHub `stark-verifier` and `plonky3` topic repos all target
  on-chain or recursive aggregation (e.g.
  `DoHoonKim8/stark-verifier`, `jwasinger/stark-verifier`), not MCU.
- Hardware wallets (Ledger, Trezor, Foundation, Coldcard) still ship
  **zero** onboard SNARK or STARK verifiers. Confirmed by absence of
  any product announcement.
- **Dual-hash FRI / Poseidon2 ∥ Blake3 hash-tower composition** for
  soundness — **no published reference**. Plonky3 supports both
  hashes individually as `StarkConfig` choices but I found no paper
  or implementation running them in sequence for hash-tower
  soundness composition.

That last absence is potentially **the strongest novelty claim in the
work**, more than the embedded port itself. Worth leading with.

---

## Recommended additions to `research/prior-art/main.typ`

Add a new section "Direct prior art (STARK side)" with the four
comparison rows:

| Work | Year | Verifier on | Field × Ext | Hash | PQ? | Measured? |
|---|---|---|---|---|---|---|
| winterfell | ongoing | server (no_std capable, unmeasured) | various | Blake3 / Rescue / etc. | yes | server only |
| zkDilithium (ePrint 2023/414) | 2023 | server | (winterfell defaults) | (winterfell) | yes (PQ creds) | server |
| RISC Zero zkVM | ongoing | server | BabyBear | Poseidon2 | yes | r6a.16xlarge |
| Succinct SP1 | ongoing | server | BabyBear | Poseidon2 | yes | r6a.16xlarge |
| **this work, Phase A-E** | **2026** | **RP2350 Cortex-M33 / Hazard3 RV32** | **BabyBear × Quartic** | **Poseidon2-BB-16 + Blake3 (dual)** | **yes (127 conj. + dual-hash)** | **MCU, both ISAs** |

The bottom row is the only one in the MCU column. That's the gap.

---

## Updates to the security-claim table notebook

The corresponding notebook
(`2026-04-30-security-claim-table.md`, drafted earlier today) needs
two amendments based on this prior-art search:

1. **Resolve open question 2**: the Phase D Goldilocks-vs-BabyBear
   "hash-bound STARK verifier" finding has **not been published
   elsewhere** in the form we measured. We can claim it as our finding
   without citation, with the framing "we expected the Phase 3.3
   fib1024 result (arithmetic-bound) to transfer; it did not, because
   PQ-Semaphore verify is hash-bound. We document the negative result
   here." No prior literature contradicts or pre-empts this.

2. **Promote the dual-hash composition** from "interesting property"
   to "lead novelty claim". The phrase "first published dual-hash
   FRI verification" needs to be in the abstract / first paragraph
   of the writeup.

---

## Open follow-ups (low-priority, do before publication)

- [ ] Search Chinese / Russian / Korean cryptography venues for
      anything similar. Limited because I don't read those
      languages — the existing search was English-only ePrint /
      arxiv / GitHub. Risk: low (these venues mirror to ePrint
      eventually) but non-zero.
- [ ] Check the proceedings of zkSummit, ZK Hack, RWC 2024-2025
      slide decks for any embedded-STARK demo that didn't make it
      into a paper. Spot-check 2-3 talk titles, low effort.
- [ ] Reach out to the Plonky3 maintainers privately (after the
      writeup goes live) to confirm we haven't missed an internal
      embedded port someone built and didn't publish.

None of these are blockers for v1 publication. They are
"insurance against a reviewer surfacing a missed citation".
