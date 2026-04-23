//! Bump-style `no_std` global allocator with watermark save/restore.
//!
//! Built as a benchmarking tool, not a production allocator. The point
//! is to make per-iteration allocator timing *deterministic* so that
//! iteration-to-iteration variance measurements stop being dominated
//! by free-list evolution inside a general-purpose heap.
//!
//! # Usage
//!
//! ```ignore
//! use zkmcu_bump_alloc::BumpAlloc;
//!
//! const HEAP_SIZE: usize = 256 * 1024;
//! #[repr(align(8))]
//! struct Arena([u8; HEAP_SIZE]);
//! static mut HEAP_MEM: Arena = Arena([0; HEAP_SIZE]);
//!
//! #[global_allocator]
//! static HEAP: BumpAlloc = BumpAlloc::new();
//!
//! fn main() {
//!     // SAFETY: HEAP_MEM is 'static and touched by nothing else after this.
//!     unsafe {
//!         HEAP.init(core::ptr::addr_of_mut!(HEAP_MEM) as *mut u8, HEAP_SIZE);
//!     }
//!     let long_lived = parse_once();
//!     let reset_point = HEAP.watermark();
//!     loop {
//!         // SAFETY: no live refs above reset_point at this point.
//!         unsafe { HEAP.reset_to(reset_point) };
//!         do_work_that_allocates(&long_lived);
//!     }
//! }
//! ```
//!
//! Limitations:
//!
//! - Individual allocations can never be freed — `dealloc` is a no-op.
//!   Memory is only reclaimed by calling [`BumpAlloc::reset_to`] with a
//!   previously-captured watermark.
//! - The reset is `unsafe`: the caller must prove no live references
//!   point above the watermark at the call site.
//! - Not suitable for firmware that holds dynamic state across
//!   iterations. Use `embedded-alloc::LlffHeap` or `TlsfHeap` instead.

#![no_std]

use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicUsize, Ordering};

/// Bump allocator with checkpoint/reset semantics.
///
/// Alloc path: lock-free CAS on the bump pointer. `dealloc` is a no-op.
/// Memory is reclaimed only by [`Self::reset_to`].
pub struct BumpAlloc {
    start: AtomicUsize,
    end: AtomicUsize,
    current: AtomicUsize,
}

impl BumpAlloc {
    /// Construct an uninitialised allocator. Must be followed by
    /// [`Self::init`] before any allocation.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            start: AtomicUsize::new(0),
            end: AtomicUsize::new(0),
            current: AtomicUsize::new(0),
        }
    }

    /// Install the backing arena.
    ///
    /// # Safety
    ///
    /// - `arena` must point to a region valid for reads and writes for
    ///   `size` bytes, for `'static`.
    /// - `size` must be non-zero.
    /// - Nothing else in the program may access the arena through any
    ///   path besides this allocator while the allocator is live.
    /// - Must be called exactly once, before any allocation happens.
    pub unsafe fn init(&self, arena: *mut u8, size: usize) {
        let start = arena as usize;
        self.start.store(start, Ordering::SeqCst);
        self.end.store(start.saturating_add(size), Ordering::SeqCst);
        self.current.store(start, Ordering::SeqCst);
    }

    /// Snapshot the current bump pointer. Pass to [`Self::reset_to`] to
    /// discard all allocations made after this point.
    #[must_use]
    pub fn watermark(&self) -> usize {
        self.current.load(Ordering::SeqCst)
    }

    /// Reset the bump pointer to a previously-captured watermark.
    ///
    /// Everything allocated above the watermark is logically freed
    /// without running destructors. The memory itself is reclaimed for
    /// subsequent allocations.
    ///
    /// # Safety
    ///
    /// The caller must ensure that no live reference of any kind exists
    /// to any byte above `watermark` at the point of this call. In
    /// particular:
    ///
    /// - All `Vec`, `Box`, `String`, `Arc`, and other allocating
    ///   containers whose backing memory came from above the watermark
    ///   must have been dropped, forgotten, or be otherwise provably
    ///   unreachable.
    /// - `Drop` impls that would have run on those values are *not*
    ///   invoked by `reset_to`. If those `Drop` impls have side effects
    ///   outside the arena (I/O, sending on a channel, etc.) those side
    ///   effects will be skipped.
    pub unsafe fn reset_to(&self, watermark: usize) {
        self.current.store(watermark, Ordering::SeqCst);
    }

    /// Bytes currently allocated from the arena.
    #[must_use]
    pub fn used_bytes(&self) -> usize {
        self.current
            .load(Ordering::SeqCst)
            .saturating_sub(self.start.load(Ordering::SeqCst))
    }

    /// Bytes remaining in the arena.
    #[must_use]
    pub fn remaining_bytes(&self) -> usize {
        self.end
            .load(Ordering::SeqCst)
            .saturating_sub(self.current.load(Ordering::SeqCst))
    }
}

impl Default for BumpAlloc {
    fn default() -> Self {
        Self::new()
    }
}

// SAFETY: all allocation bookkeeping goes through atomic CAS on `current`,
// which is the only mutable state observed from the trait methods. `alloc`
// is serialisable: concurrent callers either win the CAS and get a unique
// range, or lose and retry. `dealloc` is a no-op, which is a legal (if
// memory-wasteful) implementation of `GlobalAlloc`. The trait invariants
// "alloc returns a pointer valid for `layout` or null" and "dealloc is
// safe to no-op given matched alloc" are both upheld.
unsafe impl GlobalAlloc for BumpAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();
        let align_mask = align.wrapping_sub(1);
        let end = self.end.load(Ordering::Relaxed);

        let mut current = self.current.load(Ordering::Relaxed);
        loop {
            let Some(aligned) = current.checked_add(align_mask) else {
                return core::ptr::null_mut();
            };
            let aligned = aligned & !align_mask;
            let Some(new) = aligned.checked_add(size) else {
                return core::ptr::null_mut();
            };
            if new > end {
                return core::ptr::null_mut();
            }
            match self.current.compare_exchange_weak(
                current,
                new,
                Ordering::SeqCst,
                Ordering::Relaxed,
            ) {
                Ok(_) => return aligned as *mut u8,
                Err(actual) => current = actual,
            }
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // No-op. Use `reset_to` to reclaim memory.
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // Optimisation: if the allocation being resized is the *topmost*
        // allocation in the arena (its end address equals `current`),
        // then resizing in place just moves the bump pointer — no copy,
        // no leak. This is the Vec::push / reserve case, which is the
        // #1 reason a naive bump allocator explodes on realistic
        // workloads: each Vec growth leaks the old capacity otherwise.
        //
        // If the allocation is *not* on top (something was allocated
        // between this one and `current`), fall back to the default
        // alloc-copy path. That still leaks the old slot, but the
        // common case is handled.
        let old_size = layout.size();
        let align = layout.align();
        let ptr_addr = ptr as usize;
        let end_of_old = ptr_addr.saturating_add(old_size);

        let current = self.current.load(Ordering::SeqCst);
        if end_of_old == current {
            let end = self.end.load(Ordering::Relaxed);
            let Some(new_end) = ptr_addr.checked_add(new_size) else {
                return core::ptr::null_mut();
            };
            if new_end > end {
                return core::ptr::null_mut();
            }
            // Try to atomically move `current` to the new end. If
            // someone else allocated between our load and store, fall
            // through to the copy path.
            if self
                .current
                .compare_exchange(current, new_end, Ordering::SeqCst, Ordering::Relaxed)
                .is_ok()
            {
                return ptr;
            }
        }

        // Fallback: allocate fresh, copy, leak the old slot.
        let Ok(new_layout) = Layout::from_size_align(new_size, align) else {
            return core::ptr::null_mut();
        };
        // SAFETY: GlobalAlloc::alloc is safe to call via the trait contract.
        let new_ptr = unsafe { self.alloc(new_layout) };
        if !new_ptr.is_null() {
            // SAFETY: `ptr` is valid for reads of `old_size` (trait contract);
            // `new_ptr` is valid for writes of `new_size` (we just allocated).
            // `old_size.min(new_size)` cannot exceed either buffer.
            unsafe {
                core::ptr::copy_nonoverlapping(ptr, new_ptr, old_size.min(new_size));
            }
        }
        new_ptr
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use core::alloc::Layout;
    use std::vec;
    use std::vec::Vec;

    fn make_arena(size: usize) -> (Vec<u8>, BumpAlloc) {
        let mut buf = vec![0u8; size];
        let alloc = BumpAlloc::new();
        // SAFETY: `buf` outlives every use of `alloc` in these tests; no
        // other code touches the arena.
        unsafe {
            alloc.init(buf.as_mut_ptr(), size);
        }
        (buf, alloc)
    }

    #[test]
    fn alloc_bumps_forward_and_aligns() {
        let (_buf, alloc) = make_arena(1024);
        let layout = Layout::from_size_align(8, 8).expect("layout");
        // SAFETY: arena is initialised and sized; bump alloc has no aliasing.
        let p1 = unsafe { alloc.alloc(layout) };
        assert!(!p1.is_null());
        // SAFETY: same as above.
        let p2 = unsafe { alloc.alloc(layout) };
        assert!(!p2.is_null());
        assert_eq!(p2 as usize - p1 as usize, 8);
    }

    #[test]
    fn reset_reclaims_memory() {
        let (_buf, alloc) = make_arena(1024);
        let layout = Layout::from_size_align(64, 8).expect("layout");
        let base = alloc.watermark();
        for _ in 0..10 {
            // SAFETY: arena has capacity; no outstanding refs across iters.
            let p = unsafe { alloc.alloc(layout) };
            assert!(!p.is_null());
            // SAFETY: nothing points into the 64 bytes just allocated.
            unsafe { alloc.reset_to(base) };
        }
        assert_eq!(alloc.used_bytes(), 0);
    }

    #[test]
    fn alloc_returns_null_when_arena_exhausted() {
        let (_buf, alloc) = make_arena(16);
        let big = Layout::from_size_align(32, 1).expect("layout");
        // SAFETY: trait impl itself is safe to call; null signals oom.
        let p = unsafe { alloc.alloc(big) };
        assert!(p.is_null());
    }

    #[test]
    fn realloc_grows_in_place_on_top_of_bump() {
        // Single growing Vec scenario — each realloc should extend the
        // bump pointer without leaking.
        let (_buf, alloc) = make_arena(1024);
        let layout = Layout::from_size_align(16, 1).expect("layout");
        // SAFETY: arena has room; single-threaded.
        let p = unsafe { alloc.alloc(layout) };
        assert!(!p.is_null());
        assert_eq!(alloc.used_bytes(), 16);

        // Grow repeatedly. Each grow is top-of-bump → extend in place.
        // SAFETY: `p` is the most recent alloc; layout matches.
        let p = unsafe { alloc.realloc(p, layout, 32) };
        assert!(!p.is_null());
        assert_eq!(alloc.used_bytes(), 32);

        let l2 = Layout::from_size_align(32, 1).expect("layout");
        // SAFETY: same invariants.
        let p = unsafe { alloc.realloc(p, l2, 64) };
        assert!(!p.is_null());
        assert_eq!(alloc.used_bytes(), 64);

        // One more grow — still on top.
        let l3 = Layout::from_size_align(64, 1).expect("layout");
        // SAFETY: same.
        let p = unsafe { alloc.realloc(p, l3, 128) };
        assert!(!p.is_null());
        assert_eq!(alloc.used_bytes(), 128);
    }

    #[test]
    fn realloc_falls_back_when_not_on_top() {
        // Two allocations: A, B. Growing A (not on top) must alloc new,
        // copy, and leak the old slot — no overlap corruption.
        let (_buf, alloc) = make_arena(1024);
        let la = Layout::from_size_align(16, 1).expect("layout");
        let lb = Layout::from_size_align(16, 1).expect("layout");
        // SAFETY: arena has room.
        let a = unsafe { alloc.alloc(la) };
        assert!(!a.is_null());
        // SAFETY: arena has room.
        let b = unsafe { alloc.alloc(lb) };
        assert!(!b.is_null());
        // Write a sentinel pattern into A so we can check the copy.
        for i in 0u8..16 {
            // SAFETY: a is valid, in-bounds.
            unsafe {
                a.add(usize::from(i)).write(i);
            }
        }
        // Grow A. Not on top → fallback path.
        // SAFETY: `a` is the older of the two live allocs, `b` sits above it.
        let a2 = unsafe { alloc.realloc(a, la, 32) };
        assert!(!a2.is_null());
        assert_ne!(a2, a); // new pointer, above B
                           // Copy must preserve first 16 bytes.
        for i in 0u8..16 {
            // SAFETY: a2 is valid, in-bounds.
            let v = unsafe { a2.add(usize::from(i)).read() };
            assert_eq!(v, i);
        }
        // used_bytes accounts for A (leaked) + B + new A: 16 + 16 + 32 = 64.
        assert_eq!(alloc.used_bytes(), 64);
    }
}
