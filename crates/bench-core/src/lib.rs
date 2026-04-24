//! Shared firmware helpers for the zkmcu RP2350 benchmarks.
//!
//! Hosts the pieces that every `bench-rp2350-*` binary would otherwise
//! duplicate: clock / USB bring-up, the cycle-counter abstraction over
//! Cortex-M33 DWT and Hazard3 `mcycle`, heap tracking, stack painting,
//! and the serial-print helpers. Proof-system-specific code (verify
//! calls, vector loading, `pre_init` hooks) stays in the binary crates.
//!
//! Target is the RP2350 specifically; there is no attempt to abstract
//! over vendors or boards. When a second silicon vendor appears, this
//! crate is the place to split.

#![no_std]
// Integer math on cycle counts and microsecond conversions — same
// justification as the firmware binaries.
#![allow(clippy::integer_division)]
// Init panics are acceptable in firmware: continuing with bad hardware
// state is strictly worse than halting. See CLAUDE.md.
#![allow(clippy::panic)]

pub mod boot;
pub mod heap;
pub mod stack;
pub mod timing;
pub mod usb;

pub use boot::{init_rp2350, SYS_HZ, XTAL_HZ};
pub use heap::{TrackingLlff, TrackingTlsf};
pub use stack::{measure_stack_peak, STACK_PAINT_BYTES, STACK_PAINT_MARGIN, STACK_SENTINEL};
pub use timing::{cycles_u64, init_cycle_counter, measure_cycles};
pub use usb::{Bench, BenchConfig, Timer0};

// Re-exported so binary crates can write `fn helper<B: UsbBus>(...)`
// without pulling `usb-device` as a direct dep just for the trait.
pub use usb_device::class_prelude::UsbBus;
