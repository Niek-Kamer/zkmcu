# 2026-04-29 m33 PQ-Semaphore depth-10 — headline Plonky3 verify

## Headline

`pq_semaphore_verify`: **1049.72 ms** median across 19 iterations on
the Pico 2 W Cortex-M33 @ 150 MHz. All iterations `ok=true`.
Variance **0.029 %** (max - min over median), the tightest measurement
in the project to date.

This is the headline phase-4.0 result: depth-10 Merkle membership +
nullifier + scope binding, all evaluated against a custom AIR over
BabyBear with the audited Poseidon2-16 hash. 4-element BabyBear
digests (~124-bit collision resistance), 64 FRI queries, log_blowup=1.
Replaces the Semaphore v4 BN254/Groth16 verifier with a PQ-secure
STARK on the same silicon.

## What this falsifies

`research/reports/2026-04-29-pq-semaphore-scoping.typ` § 5 predicted
*900-1800 ms M33, point estimate 1200 ms*. Measured **1049 ms**, 12.6 %
below the point estimate, comfortably inside the band. The chain
anchor on the same day measured 492 ms and looked like the prediction
was 2x too pessimistic; the 64 queries (vs 28) and the heavier
constraint set (Merkle + scope + nullifier) push the cost back up so
the published prediction holds.

| Quantity                 | Predicted          | Measured          | Verdict        |
|--------------------------|--------------------|-------------------|----------------|
| Verify on M33            | 900-1800 ms        | 1049.72 ms        | inside (lower-middle) |
| Proof size               | 15-30 KB           | 165 KB            | over (5.5x high) |
| Heap (after parse)       | 80-140 KB          | 150 KB            | over (~7%)     |
| Variance                 | 0.05-0.15 %        | 0.029 %           | below (tightest in project) |
| Stack peak               | 4-8 KB             | not captured      | -              |

The 165 KB proof is the headline deviation. With 28 queries the proof
would be ~74 KB, still over the 30 KB upper bound but in the same
order. The scoping doc anchored proof size on a Plonky3-style AIR
with ~24 queries; the actual config uses 64 queries to hit 95-bit
conjectured security. Per-query Merkle openings dominate proof size,
linear in query count.

## What this means against Groth16/BN254

PQ-Semaphore on M33 runs at **1.91x the Groth16/BN254 verify cost**
on the same silicon (550.7 ms baseline from
`benchmarks/runs/2026-04-28-m33-bn254-rebench`). That puts it at the
lower end of the scoping doc's "PQ tax 2-3x" framing. The proof
inflates 660x (256 B → 169 KB), which is the real deployment cost —
still small enough to pin in flash on a 4 MB part.

Compare the chain anchor (492 ms): that AIR ran 11 % FASTER than
Groth16. The differentiator is query count, not field/hash. Plonky3
verify cost on this silicon is dominated by FRI per-query work and
constraint evaluation, both of which scale with security-driven
parameters rather than the AIR shape per se.

## Capture issue worth flagging

The boot line + first two fence posts (`[fp] before parse_proof`,
`[fp] after parse_proof`) came through cleanly. Subsequent fence
posts and the `[boot]` line carrying `stack_peak` and final
`heap_peak` were truncated mid-write. Root cause: `bench.write_line`
has a 1 s deadline and 20 ms flush; during `make_config` and
`build_air` the firmware does no USB poll for several hundred
milliseconds, the host-side buffer fills, the write deadline expires,
the next `[fp]` write starts but doesn't finish, then the iter loop
takes over.

Fix for next run: pump `bench.dev.poll()` once between every fence
post and inside `make_config` / `build_air`, or insert a
`bench.pace(50_000)` before each heavy section. Steady-state iter
loop is unaffected.

What we do know about memory:

- `heap_after_parse = 154 072 B` confirmed from both captures.
- `heap_peak` lower bound is 154 072 B, expected ~250-280 KB based
  on the chain anchor's 216 KB at 28 queries scaled by the 64-query
  workload. Still well inside the 384 KB heap budget.
- `stack_peak` not captured. Chain anchor measured 2.4 KB; small
  bump expected for the deeper constraint-eval recursion in the
  headline AIR but no reason to think it crosses 8 KB.

The headline cycle count is unaffected by this — `measure_cycles`
runs DWT entirely independently of USB.

## Surprises worth flagging

1. **Variance is 0.029 %.** Tightest measurement in the project, and
   it stays this tight on the heaviest workload (321 trace columns,
   64 queries, ~280 KB heap working set). The combination of
   `measure_cycles` discipline + TLSF allocator + fully deterministic
   Plonky3 verify path is doing real work on noise.

2. **Heap-bump from 256 → 384 KB was load-bearing.** The first flash
   with 256 KB heap hung on M33 silently because TLSF was returning
   null mid-verify and the alloc-error handler was panicking. RV32
   with 256 KB hung at the same point. 384 KB clears verify
   comfortably with margin to spare.

3. **Cross-ISA ratio 1.19x is the tightest in the project for any
   non-trivial workload.** M33 1049 ms vs RV32 1249 ms.
   See `2026-04-29-rv32-pq-semaphore/notes.md` for that side.

## Reproducibility

```bash
# Regenerate proof.bin + public.bin (byte-deterministic, SHA-256 stable):
just regen-vectors
# or: cargo run -p zkmcu-host-gen --release -- pq-semaphore

# Build firmware:
just build-m33-pq-semaphore

# Hand-deliver to Pi 5:
scp target/thumbv8m.main-none-eabihf/release/bench-rp2350-m33-pq-semaphore \
    pid-admin@10.42.0.30:/tmp/bench-m33-pq-semaphore.elf

# On Pi 5 with Pico in BOOTSEL:
picotool load -v -x -t elf /tmp/bench-m33-pq-semaphore.elf
cat /dev/ttyACM0
```

## Files

- `raw.log` — captured serial output, 19 + 3 iterations across two flashes.
- `result.toml` — structured results + falsification scorecard.
- This `notes.md`.
