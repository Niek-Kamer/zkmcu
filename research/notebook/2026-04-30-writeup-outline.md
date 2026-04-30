# Writeup outline — PQ-Semaphore A-E headline

**Status:** outline v1, 2026-04-30. Skeleton for the technical post that
goes simultaneously to (a) blog / Substack, (b) IACR ePrint as a report,
(c) cross-post to HN / lobste.rs / r/cryptography / r/embedded /
Plonky3 GitHub Discussions / Ethereum Magicians.

**Length target:** 4500-5500 words for the long form (ePrint PDF +
canonical blog post). 1500 words for the punchy version (HN /
Hackaday submission lead).

**Voice:** Dutch-ESL Reddit-engineer per the existing repo (Ofcourse,
wich, no em-dashes, picks sides). Same voice as the README. Honest,
technical, no breathless marketing.

---

## Title (locked)

> **Post-quantum Semaphore on a $7 microcontroller in 1.6 seconds:
> a benchmark of Plonky3 STARK verification with dual-hash
> composition on Cortex-M33 and Hazard3 RV32**

Subtitle for ePrint version:

> "First measured `no_std` Rust Plonky3 STARK verifier on a Cortex-M-
> class microcontroller. First published dual-hash (Poseidon2 ∥ Blake3)
> FRI verification with hash-tower soundness composition. Five-phase
> measurement methodology with one negative result."

---

## TL;DR (the ~1500-word HN-version body)

1. We replace the post-quantum-broken Groth16/BN254 verifier in
   Semaphore-style identity proofs with a Plonky3 STARK verifier.
2. On a Raspberry Pi Pico 2 W ($7, RP2350 with both Cortex-M33 and
   Hazard3 RV32 cores), the new verifier runs in 1.6 s on M33 and
   2.0 s on RV32 with **dual-hash (Poseidon2 ∥ Blake3) composition**,
   384 KB heap, 337 KB total proof bytes.
3. We measured every dial in the soundness budget across five phases
   (grinding, digest width, two-stage early-exit, Goldilocks alt-
   config, and dual-hash). One phase rejected its own hypothesis (the
   Goldilocks alt-config). All five are documented with predictions
   and falsifiable success criteria.
4. The headline 1.6 s number includes parse + verify of two
   independent FRI proofs sharing one statement. Cryptanalytic
   surprise on either hash family does not collapse the verifier.
5. Open hardware + open firmware + open vectors. Cargo workspace,
   `no_std` Rust, MIT-OR-Apache. Reproducible from a single
   `./reproduce.sh`.

---

## Section structure (the ~5000-word long form)

### 1. Why this exists (~400 words)

- The Semaphore protocol family ships on Groth16 / BN254. Pairings.
- Shor's algorithm breaks BN254 in O(polylog n) on a quantum
  computer. The 110-bit classical security floor is also fragile in
  the long-horizon: STNFS attacks chip away every few years.
- "Semaphore but post-quantum" is a known gap; current candidates
  are server-class STARK verifiers (zkDilithium, Polygon zkEVM
  recursion). None run on the kind of hardware that a Semaphore-style
  identity proof actually needs to live on (phones, wallets, cheap
  embedded).
- This work measures what it costs to bring the verifier all the way
  down to a $7 microcontroller while keeping post-quantum soundness.
- Lead with: "your great-grandkids in 2080 should still be able to
  verify this proof was constructed by a member of the eligible set,
  without knowing which member, and without trusting that BN254
  hasn't fallen by then."

### 2. Hardware and methodology (~600 words)

- Pico 2 W. RP2350. 150 MHz. 524 KB SRAM, 4 MB flash.
- Two cores, two ISAs: Cortex-M33 (ARMv8-M-mainline-FP, has UMAAL
  fused multiply-accumulate, single-cycle barrel rotate) and Hazard3
  (RV32IMAC, no fused multiply-accumulate, no single-instruction
  rotate). Same die. Same clock. Same memory map. Same `cargo`
  invocation, two target triples. **The cross-ISA portability story
  IS the methodology contribution.**
- All numbers measured on-device with `DWT::cycle_count()` (M33) and
  `mcycle` (RV32). 20 iterations per bench, range pct < 0.1%.
- Single source of truth: `benchmarks/runs/<date>-<slug>/result.toml`.
  Typst reads TOML natively for the paper figures; web/MDX imports
  through a typed loader for the docs site at zkmcu.dev.
- Predictions are written *before* the firmware is flashed and
  committed alongside the result. Most predictions in this work
  turned out partially or fully wrong; the writeup discloses the
  delta in every phase.

### 3. The verifier stack (~700 words)

Subsections:
- Custom AIR shape: depth-10 Merkle membership + nullifier + scope
  binding. 16 trace rows, ~332 cols. Embeds the audited Plonky3
  Poseidon2-`BabyBear`-16 columns verbatim (the audit covers the
  constraint shape, which we preserve byte-for-byte).
- Public-input layout: 24 BabyBear elements = 96 bytes wire
  (merkle_root, nullifier, signal_hash, scope_hash). Same shape as
  the v1 Semaphore protocol but PQ.
- FRI parameters: 64 queries × log_blowup=1, 16+16 grinding bits,
  digest_width=6 (post-Phase-B), 21-round Poseidon2 with audited RCs.
- Two `StarkConfig`s in v1: Poseidon2-`BabyBear`-16 (the algebraic
  leg) and Blake3-via-`SerializingHasher` (the generic leg). Both
  consume the same trace, prove the same statement, share one public-
  input blob.
- One paragraph on why we picked Plonky3 over winterfell: Plonky3's
  audited Poseidon2-`BabyBear` constants from `p3-baby-bear` reach
  the verifier byte-for-byte, so the on-device verifier inherits the
  audit without re-auditing. Winterfell would have required
  re-auditing because its Poseidon2 wasn't packaged that way.
  (Reference: spike at
  `research/notebook/2026-04-29-pq-semaphore-verifier-spike.md`.)

### 4. The five phases (~1800 words, the meat)

This is the section that earns credibility. Each phase gets ~350
words: prediction (with the bands the plan committed to), measured
result, verdict. Phase D (negative result) is the lead-bullet of the
section, framed as the credibility-building story.

#### 4.1 Phase A — grinding to 127-bit conjectured FRI

- Predicted: +0-12 ms verify cost for +32 conjectured bits.
- Measured: M33 +14.75 ms (+1.40%), RV32 +6.25 ms (+0.51%).
- Inside band. Cheap defensive move. New baseline = 127-bit
  conjectured FRI.

#### 4.2 Phase B — Merkle digest 4 → 6

- Predicted: +12-22% verify cost for raising the hash floor 124 →
  186-bit.
- Measured: +1.40% / +1.09%. Far below band.
- Why so cheap: Merkle openings live as AIR witness columns, not
  external FRI openings. Going from d=4 to d=6 widens the trace
  by ~11 cols (~3.4%), not multiplies Poseidon2 work. The plan's
  per-input absorption model was wrong; the correct model is per-
  trace-column LDE cost. This is the kind of thing on-device
  measurement teaches you that whiteboard analysis doesn't.

#### 4.3 Phase C — two-stage early exit

- Predicted: final-layer reject in 600-900 ms (vs ~1050 ms honest);
  header-byte reject < 1 ms.
- Measured: header reject 8.5 ms, final-layer reject 44 ms (14× faster
  than predicted), worst-case attacker reject (public-input desync)
  127 ms.
- Why far below band: the upstream FRI verifier checks per-round
  commit-phase PoW *before* the per-query Merkle loop, so corruption
  in the tail short-circuits after one round, not all 64.
- Practical implication for embedded availability: worst-case
  attacker DoS efficiency capped at ~9× honest verify time. Not
  a soundness change, but a real win for "this device must keep
  responding to honest proofs even under adversarial flooding".

#### 4.4 Phase D — Goldilocks × Quadratic alt-config (HYPOTHESIS REJECTED)

- The lead-bullet of this section. Honest writeup of a wrong prediction.
- Predicted: M33 600-680 ms (66% faster than `BabyBear` × Quartic, per
  Phase 3.3 fib1024 finding).
- Measured: M33 1995.66 ms (+87.2%), RV32 2700.84 ms (+112.7%).
- Why prediction failed: Phase 3.3 fib1024 was *arithmetic-bound*
  (state additions, S-boxes on a small trace). PQ-Semaphore verify
  is *hash-bound* (64 FRI queries × ~10 Merkle hops × Poseidon2
  permutations dominate). Two compounding effects: 64-bit
  Goldilocks × 32-bit MCU = 3-4× per-op base-field cost, plus
  Poseidon2-Goldilocks-16 has 22 partial rounds vs `BabyBear`-16's
  13 (1.7× more). Multiplied: ~6× per-permutation hash cost.
- Implication for the paper's 2x2 menu: Goldilocks gives a stronger
  field-side claim (no conjecture stack) but at +87% verify cost.
  Reader picks. We don't kill the `BabyBear` path.

#### 4.5 Phase E.1 — stacked dual-hash (HEADLINE)

- Predicted: M33 2200-2500 ms / RV32 2700-3000 ms (assumed dual ≈ 2 ×
  Phase B).
- Measured: M33 1611.39 ms (+51.2%), RV32 2041.78 ms (+60.8%). Below
  band.
- Why: Blake3 leg is materially cheaper than Poseidon2 leg on
  Cortex-M33. Per-leg estimates: P2 leg ~1066 ms, B3 leg ~545 ms.
  Blake3's ARX inner loop runs at native byte throughput on
  Cortex-M33 (single-cycle rotate, no field arithmetic).
  Poseidon2-`BabyBear` has to compute width-16 algebraic permutations
  for every Merkle node and FRI absorption. **The conventional
  wisdom that Poseidon2 is the embedded-friendly hash is correct
  on the prover side and wrong on the verifier side.** Worth flagging.
- Cross-ISA gap widens 1.19× → 1.27× because Hazard3 lacks single-
  instruction rotate (3-instruction emulation cost).
- This is the first published dual-hash FRI verification with hash-
  tower soundness composition. Lead claim of the paper.

### 5. Security analysis (~500 words)

- Per-phase combined-floor table (from
  `research/notebook/2026-04-30-security-claim-table.md`).
- ~180-word security paragraph (also in security-claim notebook).
- Audit-boundary disclosure (audited: Plonky3 core, Poseidon2 RCs in-
  tree, Blake3 1.8 upstream; NOT audited: custom AIR, postcard wire
  format, firmware, Blake3 type-level composition).
- Side-channel disclosure (timing in-progress, power/fault/EM not
  analyzed).
- Physical / supply-chain disclosure (no secure element, ~30 min SRAM
  dump with physical access, "convenience-grade self-custody"
  labelling).
- UMAAL asm disclosure (used by Groth16 *baseline only*; three-
  implementation differential test chain; selftest is a separate
  firmware flash, headline does not re-run at boot).

### 6. Comparison with related work (~400 words)

The table, with concrete verify times from the comparison-numbers
re-search:

| Work | Year | Verifier on | Hardware (CPU) | Verify scope | Verify time | PQ? |
|---|---|---|---|---|---:|---|
| Winterfell | ongoing | server | Intel Core i9-9980KH @ 2.4 GHz, 8c | Rescue 2^20 96-bit STARK | 2-6 ms | yes |
| zkDilithium (ePrint 2023/414) | 2023 | server | (Winterfell defaults) | PQ anon-cred STARK | server-class, ~10s | yes |
| RISC Zero zkVM | ongoing | server | r6a.16xlarge / 64 vCPU | zkVM verify (recursive) | not published | yes |
| Succinct SP1 | ongoing | server | r6a.16xlarge / GPU | zkVM verify | **explicitly excluded** from their methodology | yes |
| Plonky3 upstream | ongoing | server | x86 + AVX2/AVX-512 | various | not published | yes |
| MDPI 2024 cross-platform | 2024 | Raspberry Pi (model unspec) | ARM Cortex-A | "zk-STARK" (shape unspec) | 245 ms | yes |
| **this work** | **2026** | **RP2350 M33 (single-core, 150 MHz, no SIMD)** | **Cortex-M33** | **PQ-Semaphore d=10, 127-bit conj. + dual-hash** | **1611 ms** | **yes** |
| **this work** | **2026** | **RP2350 Hazard3 (single-core, 150 MHz, no SIMD)** | **RV32IMAC** | **same as above** | **2042 ms** | **yes** |

Honest gloss: server-class STARK verifiers run in milliseconds on
AVX-equipped hardware costing $1000+. The 1.6s on a $7 microcontroller
with no SIMD, 524 KB SRAM, and a single-issue 150 MHz pipeline is in
the same order of magnitude as a Pi-class Linux board running a
much smaller STARK, and within 300-800× of a laptop-class i9 running a
comparable Winterfell proof. The win is **not** raw verify speed — it
is that the verifier fits in the power, BOM, and silicon budget of
a hardware token at all, with no host or radio link assumed.

A separate, contribution-shaped fact worth its own paragraph: **none
of the major zkVM ecosystems (RISC Zero, SP1, Plonky3, Aztec) publish
embedded verify numbers**. Their published numbers are server-class
only. SP1 explicitly excludes verify time from its benchmark
methodology. Our row is the only one in the MCU column not because
the others tried and failed, but because the others did not measure.

(v1.5 enhancement, not a blocker: run a small Plonky3 example on a
Pi 5 to get a clean apples-to-apples Pi-class row in the same field
+ hash + query-count shape as ours. Half a day of work.)

### 7. What this is NOT (~250 words, now with prove-time disclosure)

- NOT a hardware wallet. No secure element, no tamper resistance,
  no key custody. Convenience-grade self-custody on open hardware.
- NOT an on-device prover (yet). The Plonky3 PQ-Semaphore prover is
  `std`-only and runs on the host. **Measured median host prove
  time on a modern laptop CPU is ~132 ms** (Poseidon2 leg ~105 ms +
  Blake3 leg ~27 ms, single-threaded, n=6 runs after warming the
  build cache). Memory footprint during proving is GB-class, which
  is why the prover does not fit on the Pico 2 W and is out of
  scope for this work. Realistic deployment shape: phone or laptop
  generates the proof, transmits via USB / QR / NFC to the embedded
  verifier, embedded verifier validates in 1.6 s. Host verifier on
  the same laptop runs in **6.18 ms** median; M33 ÷ laptop verify
  ratio is ~261× which is in the expected range for a 150 MHz
  single-issue MCU vs a multi-GHz wide-issue laptop CPU.
- NOT a deployable Semaphore replacement. Custom AIR has not been
  externally audited; integration with the Semaphore Protocol's
  identity commitment scheme is future work.
- NOT a claim that Poseidon2 is broken. Both legs of the dual-hash
  composition are individually trusted; the dual structure is
  defence in depth, not a vote of no-confidence in either hash.

The cross-CPU per-leg ratio is also worth noting: on the laptop
the Poseidon2 leg costs ~3.2× the Blake3 leg in verify (4.72 ms
vs 1.46 ms host). On the Pico the same ratio is ~2:1. Different
ratio on different CPUs but same direction: **Poseidon2 is more
expensive than Blake3 in the verifier role on every CPU we tested.**
That holds across 32-bit M33, 32-bit Hazard3, and a 64-bit laptop.

### 8. Reproducing the numbers (~300 words)

- `git clone`, `just check`, `just regen-vectors`, flash one of the
  bench firmware crates via picotool from BOOTSEL.
- `./reproduce.sh` (to be written in task #34) produces the headline
  number end-to-end on a Pi 5 + Pico 2 W setup.
- Bench artifacts at `benchmarks/runs/<date>-<slug>/{raw.log,
  result.toml, notes.md}`.
- 20-iter run, range_pct < 0.1% on every phase.

### 9. What's next (~200 words)

- Phase E.2 (hash-tower commitment) for smaller proof at higher
  implementation cost — not started, deferred until a user surfaces
  who needs the proof-size win.
- Threshold ML-DSA when it matures: drop into Phase E.1 verifier as
  the access-control gate, replacing the current Schnorr signing
  shape if anyone wants to use this for hardware-anchored signing.
- Custom AIR audit: would need a formal-methods or Plonky3-team
  review of the cross-row witness-column constraints and the
  conditional Merkle swap. Funded if any of the comparison-row
  projects (Succinct, RISC Zero, Polygon Zero, Aztec) want this
  upstreamed.
- The thing we will NOT do: build a hardware wallet ourselves.
  Reference design + open hardware files + writeup, not product.

### 10. Acknowledgements + links (~100 words)

- Plonky3 maintainers (Daniel Lubarov + 0xPolygon team).
- Audited Poseidon2 constants from
  `crates/zkmcu-poseidon-audit` (in-tree audit crate).
- Blake3 team (BLAKE / BLAKE2 / BLAKE3 lineage).
- Repo: github.com/Niek-Kamer/zkmcu (link)
- Docs site: zkmcu.dev (link)
- Bench artifacts: benchmarks/runs/2026-04-{29,30}-* (link)
- ePrint: this report (self-link once submitted)

---

## Tactical: order of writing

When the comparison-numbers fork returns, the section-6 table fills
in. Then write in this order:

1. Section 4 (the five phases) — meat first, structure forms around it.
2. Section 5 (security analysis) — paste the security paragraph from
   the security-claim notebook.
3. Sections 1, 2, 3, 7, 8, 9, 10 — frame around the meat.
4. TL;DR last (it summarises what's actually in the body).
5. Title last (makes you commit to a narrow claim).

This is the "write the body first, abstract last" discipline. Worth
3-4 days focused.

---

## What needs to be done BEFORE the draft starts

- [x] Prior-art re-search → `2026-04-30-prior-art-stark-side.md`
- [x] Security-claim table → `2026-04-30-security-claim-table.md`
- [x] UMAAL asm correctness disclosure → folded into security-claim
- [ ] Outside comparison numbers (in flight, fork running)
- [ ] README update to mirror the writeup (after writeup, not before)
- [ ] Reproducibility script `./reproduce.sh` (during section 8 writing)
- [ ] Tidy commit of the 21-ahead branch (before public link)
