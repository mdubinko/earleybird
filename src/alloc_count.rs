//! Optional allocation counter for performance measurement.
//!
//! When the crate is built with the `alloc-count` feature, [`Counting`] is
//! installed as the `#[global_allocator]` (see `src/eb.rs`) and every heap
//! allocation, reallocation, and deallocation bumps the atomics below. The
//! counters and helpers are *always* compiled (so call sites need no `cfg`),
//! but stay at zero unless the feature actually wires in the allocator —
//! [`enabled`] reports which.
//!
//! This gives an exact, diffable allocation count next to the `--stats` phase
//! breakdown — the right currency for clone-removal work, where wall-time is
//! noisy but "117,946 allocs -> 41,002" is unambiguous. See docs/PROFILING.md.
//!
//! Counters are process-global and monotonic; measure a region by taking a
//! [`snapshot`] before and after and subtracting.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

/// Number of `alloc` calls (fresh allocations).
pub static ALLOCS: AtomicU64 = AtomicU64::new(0);
/// Number of `dealloc` calls.
pub static DEALLOCS: AtomicU64 = AtomicU64::new(0);
/// Number of `realloc` calls (e.g. `Vec` growth). Tracked separately so a fresh
/// allocation (what most clones do) is not conflated with in-place growth.
pub static REALLOCS: AtomicU64 = AtomicU64::new(0);
/// Total bytes requested via `alloc` + `realloc` (new size).
pub static BYTES: AtomicU64 = AtomicU64::new(0);

/// Index order of the array returned by [`snapshot`].
pub const ALLOCS_IDX: usize = 0;
pub const DEALLOCS_IDX: usize = 1;
pub const REALLOCS_IDX: usize = 2;
pub const BYTES_IDX: usize = 3;

/// A global allocator that tallies activity on top of the system allocator.
pub struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = System.alloc(layout);
        if !p.is_null() {
            ALLOCS.fetch_add(1, Relaxed);
            BYTES.fetch_add(layout.size() as u64, Relaxed);
        }
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        DEALLOCS.fetch_add(1, Relaxed);
        System.dealloc(ptr, layout);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // Delegate to the system realloc (preserves in-place growth) but count it
        // as a realloc rather than an alloc, so the alloc tally stays a clean
        // measure of fresh allocations.
        let p = System.realloc(ptr, layout, new_size);
        if !p.is_null() {
            REALLOCS.fetch_add(1, Relaxed);
            BYTES.fetch_add(new_size as u64, Relaxed);
        }
        p
    }
}

/// Whether the counting allocator is actually installed (feature `alloc-count`).
/// When `false`, the counters never move and the `--stats` alloc line is omitted.
pub const fn enabled() -> bool {
    cfg!(feature = "alloc-count")
}

/// Snapshot the running counters as `[allocs, deallocs, reallocs, bytes]`.
/// Subtract two snapshots to attribute allocations to a region of work.
pub fn snapshot() -> [u64; 4] {
    [
        ALLOCS.load(Relaxed),
        DEALLOCS.load(Relaxed),
        REALLOCS.load(Relaxed),
        BYTES.load(Relaxed),
    ]
}
