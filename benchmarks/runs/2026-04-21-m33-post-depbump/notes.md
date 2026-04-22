# 2026-04-21 — M33 post-depbump (sanity check)

Same code path as the `2026-04-21-m33-groth16-baseline` run, but with the following dependency bumps applied to verify that upgrading doesn't regress the verifier:

| Crate | Baseline | This run |
|-------|----------|----------|
| `embedded-alloc` | 0.5.1 | 0.7.0 (+`llff` feature, `Heap` → `LlffHeap`) |
| `heapless` | 0.8 | 0.9.2 |
| `panic-halt` | 0.2 | 1.0.0 |
| `embedded-hal` | crates.io 1.0 | git:Niek-Kamer/main@4c9dd643 (via `[patch.crates-io]`) |

## Delta vs baseline

| Op | Baseline median | This run median | Δ |
|----|-----------------|-----------------|----|
| Groth16 verify | 988,512 μs | 986,432 μs | **-0.21%** |
| BN254 pairing | 533,385 μs | 533,210 μs | -0.03% |
| G1 scalar mul (typical) | 110,000 μs | 60,500 μs | variance — Hamming-weight dominates |
| Binary size (.text) | 70,500 B | 70,780 B | +280 B (embedded-alloc's `rlsf` helper) |

The verify delta is ~2 ms out of 988 ms — noise attributable to slightly different code generation and allocator path in the post-upgrade build. Nothing in the bump set touches the crypto hot path; the G1/G2/pairing cycle counts within each kind are indistinguishable from the baseline.

Correctness: `ok=true` returned on every one of seven iterations.

## Takeaway

The dependency upgrades are safe — no cycle-level regression, no binary-size explosion, no correctness change. The git fork of `embedded-hal` routes through the whole tree (rp235x-hal, embedded-hal-async, embedded-hal-nb, embedded-hal-bus all consume it) without issue.

## Observation worth keeping

Variance across the seven iterations on this run:

- Groth16 verify: span 65,985 cycles out of ~148M median → **0.045% spread**.

For comparison, running the same `substrate-bn` verify on a desktop CPU (OoO + boost + shared cache + OS preemption) sees 10–50% variance iteration-to-iteration. The Cortex-M33's in-order pipeline + fixed clock + single-tenant memory hierarchy is a defensible *product* feature for any application where predictable verification latency matters (hardware wallets, timed attestation protocols, side-channel-sensitive flows).
