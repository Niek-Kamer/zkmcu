#![no_std]

extern crate alloc;

// Purpose: force the winterfell crate's public surface into the embedded
// compilation graph. If this builds clean for both thumbv8m.main-none-eabihf
// and riscv32imac-unknown-none-elf, winterfell 0.13 is no_std-clean for
// verifier use and phase 3.1 becomes mostly scaffolding work rather than
// porting a crate that assumes std.
//
// This is a dep-fit test, not a functional one. We don't construct an AIR
// or call verify() with real proof bytes, that's phase 3.2 work after
// this spike's findings land in the notebook.

use winterfell::{Proof, VerifierError};

pub fn touch_api() -> Result<&'static str, &'static str> {
    // Reference the top-level types so the compiler has to resolve them
    // (and their transitive trait bounds) through the whole dep closure.
    let _size_of_proof = core::mem::size_of::<Proof>();
    let _size_of_err = core::mem::size_of::<VerifierError>();
    Ok("winterfell imported")
}
