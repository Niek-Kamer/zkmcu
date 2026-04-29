//! No-op spike: pulls in the Plonky3 verifier-side dep tree.
//! Goal is solely to test that the dep closure builds no_std on the
//! target microcontroller triples. No verifier wiring yet.

#![no_std]

extern crate alloc;

pub use p3_air;
pub use p3_baby_bear;
pub use p3_challenger;
pub use p3_commit;
pub use p3_field;
pub use p3_fri;
pub use p3_matrix;
pub use p3_merkle_tree;
pub use p3_poseidon2;
pub use p3_symmetric;
pub use p3_uni_stark;
