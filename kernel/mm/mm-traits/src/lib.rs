//! OXIDE Memory Management Traits
//!
//! Defines interfaces for memory allocators and page table operations.

#![no_std]

use core::sync::atomic::{AtomicU64, Ordering};
use os_core::PhysAddr;

/// Frame allocator trait
///
/// Uses `&self` to support implementations with interior mutability (locks).
pub trait FrameAllocator {
    /// Allocate a single physical frame
    fn alloc_frame(&self) -> Option<PhysAddr>;

    /// Free a physical frame
    fn free_frame(&self, addr: PhysAddr);

    /// Allocate contiguous frames
    fn alloc_frames(&self, count: usize) -> Option<PhysAddr>;

    /// Free contiguous frames
    fn free_frames(&self, addr: PhysAddr, count: usize);
}

// ============================================================================
// — CrashBloom: Frame Watchdog — catches use-after-free on critical frames.
//
// During fork, the child PML4 frame is allocated and must NOT be freed until
// the fork completes. If someone (process exit on another CPU, OOM killer,
// stale reference) frees it, the buddy allocator recycles it. The next
// alloc_frame() in the fork walk returns the PML4 frame as a PT, clearing
// it destroys PML4[256] (kernel mapping), triple fault, game over.
//
// The watchdog: fork sets the PML4 address, buddy allocator checks before
// every free. If it matches, log the violation and SKIP the free. The
// free is the bug — preventing it keeps the system alive long enough to
// diagnose who's doing it.
// ============================================================================

/// — CrashBloom: Protected frame address. 0 = no watch active.
pub static WATCHED_FRAME: AtomicU64 = AtomicU64::new(0);

/// Set the watched frame address. The buddy allocator will refuse to free
/// this frame and log a diagnostic if anyone tries.
pub fn set_frame_watch(phys: u64) {
    WATCHED_FRAME.store(phys, Ordering::Release);
}

/// Clear the watched frame.
pub fn clear_frame_watch() {
    WATCHED_FRAME.store(0, Ordering::Release);
}

/// Check if an address matches the watched frame. Returns true if it
/// matches (caller should abort the free and log the violation).
pub fn is_frame_watched(phys: u64) -> bool {
    let watched = WATCHED_FRAME.load(Ordering::Acquire);
    watched != 0 && watched == phys
}
