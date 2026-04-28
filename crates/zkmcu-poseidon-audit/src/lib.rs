//! Poseidon-BabyBear parameter audit for the PQ-Semaphore milestone.
//!
//! # Why this crate exists
//!
//! Two prior Poseidon impls live in the workspace, both with caveats:
//!
//! - `zkmcu-poseidon-circuit` (BN254, `t=3, α=5, 8+57 rounds`): explicit
//!   sizing-only placeholder, ARK zeroed, MDS placeholder. Marked NOT
//!   cryptographically sound in its own docstring. Gets retired during this
//!   milestone.
//! - `zkmcu-verifier-stark::poseidon_threshold` (`BabyBear`, `t=2, α=7,
//!   24-full-round`): real impl, ~64-bit Grobner-basis security. Designed
//!   for a range-proof use case where forgery does not require preimage
//!   inversion. Not appropriate for general 2-to-1 Merkle hashing.
//!
//! Neither fits PQ-Semaphore. We need a Poseidon2-`BabyBear` (eprint
//! 2023/323, the modern external+internal layer variant) at the canonical
//! compression-mode width with:
//!
//! 1. **Width** wide enough for 2-to-1 Merkle hashing in compression mode
//!    at 128-bit conjectured security against algebraic attacks. Plonky3
//!    and the canonical sage param script both fix this at `t = 16` for
//!    `BabyBear` compression.
//! 2. **Round count** derived from the cryptanalytic bounds in the
//!    Poseidon papers (statistical, interpolation, Groebner-1/2/3, plus
//!    the 2023/537 addition), encoded in
//!    `vendor/poseidon2/poseidon2_rust_params.sage`. We re-run the
//!    derivation, not assume the published numbers.
//! 3. **External MDS matrix** that is provably MDS over `BabyBear`
//!    (every minor nonzero), not a circulant we trust by inspection.
//! 4. **Internal diagonal** that satisfies the Poseidon2 paper's
//!    invertibility + irreducibility conditions.
//! 5. **Round constants** that are byte-for-byte reproducible from a
//!    documented seed, so an external auditor can re-derive them.
//!
//! # What this crate produces
//!
//! Each module is the audit step it is named after.
//!
//! - [`params`]: target parameter set with derivation references.
//! - [`mds`]: brute-force MDS-ness check, all 69 minors of `M_4` over
//!   `BabyBear`.
//! - [`internal_layer`]: `V` vector reconstructed from first principles,
//!   `M_I = J + diag(V)`.
//! - [`subspace_trail`]: Faddeev-LeVerrier characteristic polynomial plus
//!   Rabin irreducibility test for `M_I^k`, `k = 1..R_P`.
//! - [`round_numbers`]: independent port of `poseidon2_rust_params.sage`;
//!   recovers Plonky3's published `(R_F, R_P)` from the cryptanalytic
//!   bounds.
//! - [`perm`]: reference Poseidon2 permutation in raw `u64` arithmetic
//!   (deliberately not using `zkmcu_babybear::BaseElement`, so the audit
//!   stays independent of the field crate's Montgomery impl).
//! - [`poly`]: `F_p[x]` arithmetic backing [`subspace_trail`].
//! - `tests/perm_diff.rs`: byte-identical diff vs Plonky3's
//!   `Poseidon2BabyBear` on the published constants; 9 adversarial inputs
//!   plus a 200-input deterministic LCG stress run.

pub mod internal_layer;
pub mod mds;
pub mod params;
pub mod perm;
pub mod poly;
pub mod round_numbers;
pub mod subspace_trail;
