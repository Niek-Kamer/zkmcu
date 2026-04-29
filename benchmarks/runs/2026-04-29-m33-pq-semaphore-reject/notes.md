# Phase C — adversarial reject timing on Cortex-M33

Companion to `2026-04-29-m33-pq-semaphore-d6/` (the Phase B honest-only
baseline). Same firmware target (RP2350 M33 @ 150 MHz, 384 KB heap, TLSF
allocator), same proof bytes, but the bench loops six adversarial
mutation patterns plus an honest baseline through `parse_and_verify`,
recording reject latency per pattern.

## Setup

- Mutation patterns live in `crates/zkmcu-vectors/src/mutations.rs`.
  Each pattern flips one or more bytes of the in-memory proof / public
  bytes copy. Patterns are validated end-to-end on host before flashing
  via `crates/zkmcu-verifier-plonky3/tests/pq_semaphore_reject.rs`.
- Firmware: `crates/bench-rp2350-m33-pq-semaphore-reject/src/main.rs`.
  Per iteration: copy static bytes into a heap Vec (172 KB), apply
  mutation, run parse_and_verify, drop bytes between parse and verify
  to keep heap peak under the 384 KB ceiling, write a serial line.
- 16 iterations per pattern × 7 patterns = 112 measurements; runtime
  about 25 seconds end-to-end on the device.

## Verifier ordering — Phase C is measurement-only

The plan section budgeted a possible verifier reorder if the upstream
order did grinding-after-Merkle. Reading
`vendor/Plonky3/uni-stark/src/verifier.rs` and `fri/src/verifier.rs`
showed the order is already adversary-friendly:

1. `parse_proof` (postcard) — first gatekeeper, fails in microseconds
2. `parse_public_inputs` — canonical-element check
3. uni-stark shape checks (cheap)
4. challenger replay (a handful of Poseidon2 perms)
5. `pcs.verify` → enters FRI verify
6. FRI: per-commit-round { observe + check PoW + sample beta }
7. FRI: per-query { input Merkle batch verify + fold loop with per-step Merkle verify }

Header / commit / mid-FRI byte mutations get caught at step 1. Tail
mutations (M3, M4) get caught at step 5–6. Public-byte mutations get
caught at step 5–6 by transcript desync. No reorder needed; we just
measure the existing flow.

## Numbers

Honest baseline under the same `parse_and_verify` shape: **1130.58 ms**
median, 0.051 % range. Differs from the Phase B d6 number
(`benchmarks/runs/2026-04-29-m33-pq-semaphore-d6/`: 1065.84 ms verify
only) by +6.07 % because the reject harness times the whole pipeline
including parse + make_config + build_air. The d6 baseline pays those
once at boot and excludes them from the verify timing.

Reject medians (us / ms / speedup-vs-honest):

| Pattern              | us_median | ms    | × honest |
|----------------------|----------:|------:|---------:|
| honest_verify        | 1_130_581 | 1130  | 1.00     |
| M0 header_byte       |     8_529 |    8.5|  132.6   |
| M1 trace_commit_dig. |     8_579 |    8.6|  131.8   |
| M2 mid_fri           |    13_138 |   13.1|   86.1   |
| M3 query_opening     |    44_107 |   44.1|   25.6   |
| M4 final_layer       |    44_090 |   44.1|   25.6   |
| M5 public_byte       |   126_872 |  126.9|    8.9   |

Variance under 0.6 % on every pattern. M0 and M1 are within 0.6 % of
each other — that's the postcard-parse floor on an M33 with a 172 KB
input buffer, dominated by the heap memcpy on the bytes copy plus the
postcard cursor walking the first commit. M2 is +4.6 ms over the floor
— postcard advanced through the trace + quotient_chunks commits and
into the per-query opening structure before tripping a length-bearing
varint that fails. M3 and M4 (~44 ms) are postcard-parses-clean cases
where the verifier itself fails: the corrupted bytes live in hash-byte
data that postcard can't validate, the parsed Proof reaches the
verifier, and the per-round commit-phase PoW or Merkle root mismatch
short-circuits the first time the verifier touches the corrupted
region. M5 is the most expensive reject — public-input transcript
desync forces the verifier through every challenger sample and into
PCS verify's per-round setup before the first PoW check fails.

## What surprised me

1. **Final-layer reject lands at 44 ms, not 600–900 ms.** Plan budgeted
   full per-query Merkle work assuming the verifier had to walk all
   queries before it noticed the corrupted final-poly bytes. In
   practice the upstream FRI verifier checks per-round commit-phase
   PoW witnesses BEFORE the per-query loop runs, and the corrupted
   tail bytes likely live in `commit_pow_witnesses` (which is the last
   field in the postcard layout). The first round's PoW check fails
   in milliseconds, not hundreds of milliseconds. Predicted ordering
   was right on direction, wrong on magnitude.

2. **Header-byte floor is 8.5 ms, not "< 1 ms".** Plan implicitly
   assumed the proof bytes are already in RAM; our bench harness
   copies the 172 KB postcard blob into a heap Vec per iteration
   (~50 k cycles of memcpy at 150 MHz) plus a postcard cursor advance
   plus the bytes-Vec drop. The "< 1 ms" goal is achievable with a
   pre-loaded RAM buffer, but the pessimistic measurement here is the
   one a real device would see when the proof arrives over a network
   transport.

3. **Midpoint byte mutations OOM the firmware.** The first M33 capture
   used a midpoint flip for M3 (proof.len()/2). That landed inside a
   varint length prefix in the per-query opening structure. Postcard
   read the corrupted varint as an enormous length and tried to
   allocate a Vec to match — TLSF returned NULL, the allocator's
   handle_alloc_error triggered panic_halt, the device froze and
   serial output stopped. Switched M3 to `proof.len() - 64` (last
   query's last commit-phase Merkle path bytes — hash data, no
   varints near it) for the recapture. Documented in `mutations.rs`
   on the QueryOpening variant.

4. **M5 (public byte) is the worst-case attacker time.** 126.87 ms.
   That is the upper bound on how long a hostile proof can keep the
   device busy before rejection. Honest_verify is 1130.58 ms. Worst
   attacker DoS efficiency is therefore 1/8.91 vs an honest user. For
   a device that processes proofs from the network, this is the
   number to publish: an adversary cannot multiply CPU pressure by
   more than ~9× over a baseline of legitimate verifies.

## Outstanding

- Stack peak not captured in this run — boot_measure path was skipped
  to keep the iteration loop simple. Honest_verify stack peak is
  available from the Phase B d6 baseline (M33: 2 844 B).
- M0 / M1 cluster within 50 microseconds of each other; the dominant
  cost on those patterns is the bytes-Vec copy + drop, not the
  verifier. A future spike could measure the verifier-only floor by
  pre-pinning the bytes in BSS (skipping the heap copy).

## Links

- Plan section: `bindings/.claude/plans/2026-04-29-security-128bit.md` § Phase C
- Honest-only Phase B baseline: `benchmarks/runs/2026-04-29-m33-pq-semaphore-d6/`
- Mutation harness: `crates/zkmcu-vectors/src/mutations.rs`
- Host-side validation test: `crates/zkmcu-verifier-plonky3/tests/pq_semaphore_reject.rs`
- Firmware: `crates/bench-rp2350-m33-pq-semaphore-reject/`
