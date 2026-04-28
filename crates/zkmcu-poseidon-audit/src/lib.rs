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
//! - [`params`]: target parameter set with derivation references.
//! - `mds`: MDS-ness proof for the chosen matrix.
//! - `perm`: reference Poseidon permutation over [`zkmcu_babybear::BaseElement`].
//! - `tests/differential.rs`: byte-for-byte agreement with Plonky3's
//!   `p3-poseidon2` over millions of inputs.
//! - `tests/grobner.rs`: round-count vs algebraic-attack bound check.
//! - Eventually a Typst writeup landed under `research/reports/` and
//!   surfaced on the public site.
//!
//! # Status
//!
//! Scaffold. Parameters are placeholders pending derivation in the next
//! milestone slice.

pub mod internal_layer;
pub mod mds;
pub mod params;
pub mod perm;
pub mod poly;
pub mod round_numbers;
pub mod subspace_trail;
