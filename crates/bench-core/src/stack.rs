//! Stack-painting peak measurement.
//!
//! Writes a sentinel pattern to a 64 KB window just below the current
//! stack pointer, runs the supplied closure, then scans the window
//! upward to find the deepest address the closure's frames touched.
//! The painted region sits at least `STACK_PAINT_MARGIN` bytes below
//! the measuring function's own frame so the measurement itself can't
//! clobber the sentinel it is about to scan.

use crate::timing::measure_cycles;

pub const STACK_SENTINEL: u32 = 0xDEAD_BEEF;
pub const STACK_PAINT_BYTES: usize = 64 * 1024;
pub const STACK_PAINT_MARGIN: usize = 512;

#[inline]
fn current_sp() -> usize {
    let sp: usize;
    // SAFETY: reading the stack pointer has no side effects and
    // produces a valid address owned by the current execution context.
    unsafe {
        #[cfg(target_arch = "arm")]
        core::arch::asm!(
            "mov {sp}, sp",
            sp = out(reg) sp,
            options(nomem, nostack, preserves_flags),
        );
        #[cfg(target_arch = "riscv32")]
        core::arch::asm!(
            "mv {sp}, sp",
            sp = out(reg) sp,
            options(nomem, nostack, preserves_flags),
        );
    }
    sp
}

/// Paint the stack, run `f`, scan the paint window, return
/// `(f's result, peak depth in bytes, cycles consumed by f)`.
///
/// `#[inline(never)]` keeps the measurement frame's relationship to
/// `f`'s frames stable across builds; without it the optimiser can
/// collapse everything into the caller and the sentinel window ends
/// up painted over live frames.
#[inline(never)]
pub fn measure_stack_peak<F, R>(f: F) -> (R, Option<usize>, u64)
where
    F: FnOnce() -> R,
{
    let sp = current_sp();
    let paint_top = (sp - STACK_PAINT_MARGIN) & !3usize;
    let paint_bottom = paint_top - STACK_PAINT_BYTES;

    let mut addr = paint_bottom;
    while addr < paint_top {
        // SAFETY: `addr` is a 4-byte-aligned address strictly below the
        // current SP by at least STACK_PAINT_MARGIN bytes. The region
        // is inside the linker-reserved stack allocation, owned
        // exclusively by this execution context, and not backed by
        // live frames.
        #[allow(clippy::as_conversions)]
        unsafe {
            (addr as *mut u32).write_volatile(STACK_SENTINEL);
        }
        addr += 4;
    }

    let (result, cycles) = measure_cycles(f);

    let mut addr = paint_bottom;
    while addr < paint_top {
        // SAFETY: reading the same region we painted above.
        #[allow(clippy::as_conversions)]
        let val = unsafe { (addr as *const u32).read_volatile() };
        if val != STACK_SENTINEL {
            // Add the margin back — those bytes are part of the frame
            // chain that we deliberately didn't paint to avoid
            // clobbering the measuring function's own frame.
            return (result, Some(paint_top - addr + STACK_PAINT_MARGIN), cycles);
        }
        addr += 4;
    }

    (result, None, cycles)
}
