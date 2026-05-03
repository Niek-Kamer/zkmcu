#!/usr/bin/env python3
"""Regenerate fuzz seed corpora for the four PQ-Semaphore targets.

Mirrors crates/zkmcu-vectors/src/mutations.rs (M0-M5) so the seed
files stay in lock-step with the on-device reject benchmark patterns.
Run from repo root: `python3 fuzz/scripts/regen_pq_semaphore_seeds.py`.
"""

from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DATA = ROOT / "crates" / "zkmcu-vectors" / "data" / "pq-semaphore-d10-dual"
SEEDS = ROOT / "fuzz" / "seeds"

PROOF_P2 = (DATA / "proof_p2.bin").read_bytes()
PROOF_B3 = (DATA / "proof_b3.bin").read_bytes()
PUBLIC = (DATA / "public.bin").read_bytes()


def xor_at(buf: bytes, offset: int, mask: int) -> bytes:
    if offset >= len(buf):
        return buf
    return buf[:offset] + bytes([buf[offset] ^ mask]) + buf[offset + 1 :]


# Mutation table mirrors mutations.rs::Mutation::apply.
PROOF_MUTATIONS = {
    "m0_header_byte": lambda p: xor_at(p, 0, 0xFF),
    "m1_trace_commit_digest": lambda p: xor_at(p, 64, 0xFF),
    "m2_mid_fri": lambda p: xor_at(p, 1024, 0xFF),
    "m3_query_opening": lambda p: xor_at(p, len(p) - 64, 0xFF),
    "m4_final_layer": lambda p: xor_at(p, len(p) - 1, 0xFF),
}
PUBLIC_MUTATION = ("m5_public_byte", lambda pi: xor_at(pi, 0, 0x01))


def write_seed(target: str, name: str, blob: bytes) -> None:
    out = SEEDS / target / name
    out.write_bytes(blob)


def regen_proof_target(target: str, canonical: bytes) -> None:
    write_seed(target, "canonical", canonical)
    for name, mut in PROOF_MUTATIONS.items():
        write_seed(target, name, mut(canonical))


def regen_public_target() -> None:
    target = "pq_semaphore_parse_public"
    write_seed(target, "canonical", PUBLIC)
    name, mut = PUBLIC_MUTATION
    write_seed(target, name, mut(PUBLIC))


def encode_dual_bundle(p2: bytes, b3: bytes, public: bytes) -> bytes:
    return (
        len(p2).to_bytes(4, "little")
        + p2
        + len(b3).to_bytes(4, "little")
        + b3
        + public
    )


def regen_dual_target() -> None:
    target = "pq_semaphore_dual_parse_and_verify"
    write_seed(target, "canonical", encode_dual_bundle(PROOF_P2, PROOF_B3, PUBLIC))
    # Apply each proof mutation to BOTH legs (matches mutations.rs intent:
    # an attacker corrupts one proof bundle; either leg can be the target).
    for name, mut in PROOF_MUTATIONS.items():
        write_seed(target, f"{name}_p2", encode_dual_bundle(mut(PROOF_P2), PROOF_B3, PUBLIC))
        write_seed(target, f"{name}_b3", encode_dual_bundle(PROOF_P2, mut(PROOF_B3), PUBLIC))
    name, mut = PUBLIC_MUTATION
    write_seed(target, name, encode_dual_bundle(PROOF_P2, PROOF_B3, mut(PUBLIC)))


def main() -> None:
    regen_proof_target("pq_semaphore_parse_proof_p2", PROOF_P2)
    regen_proof_target("pq_semaphore_parse_proof_b3", PROOF_B3)
    regen_public_target()
    regen_dual_target()
    print("seed corpora regenerated under", SEEDS)


if __name__ == "__main__":
    main()
