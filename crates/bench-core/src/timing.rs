//! Cycle-counter abstraction across Cortex-M33 DWT and Hazard3 `mcycle`.
//!
//! The two ISAs expose very different counters:
//!
//! * Cortex-M33: `DWT::CYCCNT`, 32-bit, wraps every ~28 s at 150 MHz.
//!   Disabled by default (`DCB.TRCENA = 0`); enable once at boot.
//! * Hazard3 (RV32IMAC): `mcycle` + `mcycleh`, paired 32-bit CSRs
//!   forming a 64-bit counter. `mcountinhibit[CY] = 1` at reset so the
//!   counter is held at zero until explicitly cleared.
//!
//! [`measure_cycles`] is the public measurement entry point; it handles
//! the 32-bit wrap on Cortex-M correctly and returns a 64-bit delta that
//! every caller can feed into us/ms conversion uniformly. [`cycles_u64`]
//! is a plain read intended only for seed entropy — on Cortex-M it
//! zero-extends the u32 counter, so it must not be used to compute a
//! delta across wraps.

#[cfg(target_arch = "arm")]
type RawCycles = u32;
#[cfg(target_arch = "riscv32")]
type RawCycles = u64;

/// Enable the cycle counter for this core. Call exactly once at boot
/// before any measurement is taken.
#[cfg(target_arch = "arm")]
pub fn init_cycle_counter() {
    let mut cp = cortex_m::Peripherals::take().expect("cortex-m peripherals once");
    cp.DCB.enable_trace();
    cp.DWT.enable_cycle_counter();
}

#[cfg(target_arch = "riscv32")]
pub fn init_cycle_counter() {
    // SAFETY: writing `mcountinhibit` in M-mode is always legal on
    // Hazard3 and has no side effects other than (un)inhibiting the HPM
    // counters, which is the whole point.
    unsafe {
        core::arch::asm!(
            "csrw mcountinhibit, zero",
            options(nomem, nostack, preserves_flags),
        );
    }
}

#[cfg(target_arch = "arm")]
#[inline]
fn raw_cycles() -> RawCycles {
    cortex_m::peripheral::DWT::cycle_count()
}

#[cfg(target_arch = "riscv32")]
#[inline]
fn raw_cycles() -> RawCycles {
    // SAFETY: reading machine-mode CSRs has no side effects or
    // preconditions in M-mode, which is where bare-metal Rust runs.
    unsafe {
        let mut hi: u32;
        let mut lo: u32;
        let mut hi2: u32;
        loop {
            core::arch::asm!(
                "csrr {hi}, mcycleh",
                "csrr {lo}, mcycle",
                "csrr {hi2}, mcycleh",
                hi = out(reg) hi,
                lo = out(reg) lo,
                hi2 = out(reg) hi2,
                options(nomem, nostack, preserves_flags),
            );
            if hi == hi2 {
                return (u64::from(hi) << 32) | u64::from(lo);
            }
        }
    }
}

// `u64::from` is not const-stable on MSRV 1.82, so the ARM variant
// can't be a `const fn` even though the RISC-V sibling can.
#[allow(clippy::missing_const_for_fn)]
#[cfg(target_arch = "arm")]
#[inline]
fn delta(t0: RawCycles, t1: RawCycles) -> u64 {
    // Subtract at u32 width so the wrap at 2^32 is captured correctly,
    // then zero-extend. Valid for any interval under ~28 s at 150 MHz.
    u64::from(t1.wrapping_sub(t0))
}

#[cfg(target_arch = "riscv32")]
#[inline]
const fn delta(t0: RawCycles, t1: RawCycles) -> u64 {
    t1.wrapping_sub(t0)
}

/// Read the cycle counter, zero-extended to `u64`.
///
/// Suitable as an entropy source for seeding a per-iteration scalar;
/// not suitable for computing a delta across the 32-bit DWT wrap — use
/// [`measure_cycles`] for that.
#[inline]
pub fn cycles_u64() -> u64 {
    #[cfg(target_arch = "arm")]
    {
        u64::from(raw_cycles())
    }
    #[cfg(target_arch = "riscv32")]
    {
        raw_cycles()
    }
}

/// Run `f`, returning its result paired with the elapsed cycle count.
/// Handles the Cortex-M33 32-bit wrap via a `wrapping_sub` at the
/// native width before zero-extending.
#[inline]
pub fn measure_cycles<F, R>(f: F) -> (R, u64)
where
    F: FnOnce() -> R,
{
    let t0 = raw_cycles();
    let r = f();
    let t1 = raw_cycles();
    (r, delta(t0, t1))
}
