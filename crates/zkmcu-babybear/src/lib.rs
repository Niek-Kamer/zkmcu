//! `BabyBear`, a STARK-friendly 31-bit prime field for 32-bit microcontrollers.
//!
//! Modulus `p = 15 * 2^27 + 1 = 2_013_265_921` (a Proth prime with two-adicity
//! 27). Every `BaseElement` is stored in Montgomery form with `R = 2^32`, so
//! the whole element fits in a single `u32` and every multiplication costs one
//! `u32 * u32 -> u64` plus the CIOS-style reduction. On Cortex-M this maps to
//! a single `UMULL`; on RV32 it maps to `MUL` + `MULHU`.
//!
//! The point of this crate is the cross-ISA benchmark phase 3.3: the
//! `zkmcu-verifier-stark` Fibonacci AIR can be re-instantiated over `BaseElement`
//! (plus the quartic extension wich comes in a follow-up) and compared to the
//! phase 3.2 Goldilocks numbers on the same silicon, same allocator, same hasher.

#![no_std]

extern crate alloc;

mod field;

#[cfg(test)]
mod tests;

pub use field::{BaseElement, M};
