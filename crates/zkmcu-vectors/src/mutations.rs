//! Adversarial mutation patterns for the PQ-Semaphore proof.
//!
//! Each pattern flips bytes in either the proof blob or the public-inputs blob.
//! The firmware reject benchmark loops over [`ALL`] and times how long the
//! verifier takes to detect the corruption. The patterns probe every stage of
//! the verifier's reject path: postcard parse, public-input canonicality,
//! transcript-bound shape checks, FRI commit-phase Merkle openings, query-time
//! Merkle openings, and the final-poly check.

#![allow(missing_docs)]

/// One adversarial perturbation of `(proof, public)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mutation {
    /// No mutation: the honest-baseline accept path.
    None,
    /// Flip the first byte of the proof. Hits postcard's varint header → parse fails in microseconds.
    HeaderByte,
    /// Flip a byte 64 bytes in. Lands inside the trace commitment digest → Merkle batch verify fails the first time the verifier touches the commit.
    TraceCommitDigest,
    /// Flip a byte 1 KiB in. Lands in the FRI commit-phase data (commitments / final poly area) → mid-FRI Merkle reject.
    MidFri,
    /// Flip a byte at the proof midpoint. Deep inside the per-query openings → late reject in the query loop.
    QueryOpening,
    /// Flip the last byte of the proof. Hits the very last opening / final poly coefficient → near-honest reject time.
    FinalLayer,
    /// Flip the low bit of the first public-input byte. Element stays canonical (< p) so parse passes; transcript desyncs during challenger replay → rejects when the recomputed commitment fails to match.
    PublicInputByte,
}

/// All adversarial patterns plus the honest baseline, in firmware-loop order.
///
/// `Mutation::None` is included so a bench harness can iterate across every
/// case in one loop, recording one `[bench.*]` block per name.
pub const ALL: [Mutation; 7] = [
    Mutation::None,
    Mutation::HeaderByte,
    Mutation::TraceCommitDigest,
    Mutation::MidFri,
    Mutation::QueryOpening,
    Mutation::FinalLayer,
    Mutation::PublicInputByte,
];

impl Mutation {
    /// Stable identifier used in serial output / `result.toml` keys.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::None => "honest_verify",
            Self::HeaderByte => "reject_M0_header_byte",
            Self::TraceCommitDigest => "reject_M1_trace_commit_digest",
            Self::MidFri => "reject_M2_mid_fri",
            Self::QueryOpening => "reject_M3_query_opening",
            Self::FinalLayer => "reject_M4_final_layer",
            Self::PublicInputByte => "reject_M5_public_byte",
        }
    }

    /// Apply the mutation in place. `proof` is the postcard-encoded proof
    /// bytes; `public` is the 96-byte little-endian public-inputs blob.
    /// Out-of-bounds offsets are silently skipped — that situation indicates
    /// a vector regenerator has shrunk the proof, which the bench will
    /// surface on its own when the unmutated case stops verifying.
    pub fn apply(self, proof: &mut [u8], public: &mut [u8]) {
        match self {
            Self::None => {}
            Self::HeaderByte => xor_at(proof, 0, 0xff),
            Self::TraceCommitDigest => xor_at(proof, 64, 0xff),
            Self::MidFri => xor_at(proof, 1024, 0xff),
            Self::QueryOpening => {
                // Halve via right shift to dodge clippy::integer_division — the
                // bench harness opts out of that lint at crate level, but this
                // helper module compiles under the default workspace lints.
                let mid = proof.len() >> 1;
                xor_at(proof, mid, 0xff);
            }
            Self::FinalLayer => {
                let last = proof.len().saturating_sub(1);
                xor_at(proof, last, 0xff);
            }
            // Flip the low bit of public[0]. BabyBear elements are 4-byte
            // little-endian < p, and p = 0x78000001, so toggling bit 0 of byte 0
            // never crosses the canonical bound — parse_public_inputs still
            // accepts and the failure shows up later in the transcript.
            Self::PublicInputByte => xor_at(public, 0, 0x01),
        }
    }
}

#[inline]
fn xor_at(buf: &mut [u8], offset: usize, mask: u8) {
    if let Some(b) = buf.get_mut(offset) {
        *b ^= mask;
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::{Mutation, ALL};

    #[test]
    fn none_is_identity() {
        let mut p = [1_u8, 2, 3];
        let mut pi = [4_u8, 5];
        Mutation::None.apply(&mut p, &mut pi);
        assert_eq!(p, [1, 2, 3]);
        assert_eq!(pi, [4, 5]);
    }

    #[test]
    fn header_byte_flips_proof_zero() {
        let mut p = [0x55_u8, 0x55, 0x55];
        let mut pi = [0x55_u8];
        Mutation::HeaderByte.apply(&mut p, &mut pi);
        assert_eq!(p, [0xaa, 0x55, 0x55]);
        assert_eq!(pi, [0x55]);
    }

    #[test]
    fn final_layer_flips_proof_last() {
        let mut p = [0x55_u8; 8];
        let mut pi = [0x55_u8];
        Mutation::FinalLayer.apply(&mut p, &mut pi);
        assert_eq!(p[..7], [0x55; 7]);
        assert_eq!(p[7], 0xaa);
        assert_eq!(pi, [0x55]);
    }

    #[test]
    fn query_opening_flips_proof_midpoint() {
        let mut p = [0x55_u8; 8];
        let mut pi = [0x55_u8];
        Mutation::QueryOpening.apply(&mut p, &mut pi);
        let mut expected = [0x55_u8; 8];
        expected[4] = 0xaa;
        assert_eq!(p, expected);
        assert_eq!(pi, [0x55]);
    }

    #[test]
    fn public_input_byte_keeps_canonical() {
        // BabyBear prime is 0x78000001. Toggling bit 0 of any byte 0 value < p
        // can never push it above 0x78000001 because the high byte is byte 3.
        let mut p = [0_u8; 4];
        let mut pi = [0x00_u8, 0x00, 0x00, 0x00];
        Mutation::PublicInputByte.apply(&mut p, &mut pi);
        assert_eq!(pi, [0x01, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn out_of_bounds_offsets_are_silent() {
        // A 4-byte proof: TraceCommitDigest (offset 64), MidFri (1024) and
        // QueryOpening (midpoint 2) — only QueryOpening lands.
        for m in [
            Mutation::TraceCommitDigest,
            Mutation::MidFri,
            Mutation::FinalLayer,
        ] {
            let mut p = [0_u8; 4];
            let mut pi = [0_u8; 0];
            m.apply(&mut p, &mut pi);
        }
    }

    #[test]
    fn names_are_unique() {
        // `result.toml` keys are derived from `name()`; collisions would
        // overwrite earlier blocks.
        let names: Vec<&str> = ALL.iter().map(|m| m.name()).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len());
    }
}
