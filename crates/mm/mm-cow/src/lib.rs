//! Copy-on-Write (COW) page tracking
//!
//! Provides reference counting for physical frames to support COW semantics.
//! When a frame is shared (e.g., after fork), its reference count is > 1.
//! On write fault, the page is copied if count > 1, or made writable if count == 1.

#![no_std]

extern crate alloc;

use alloc::collections::BTreeMap;
use os_core::PhysAddr;
use spin::RwLock;

/// Frame size (4KB)
const FRAME_SIZE: usize = 4096;

/// COW frame tracker
///
/// Tracks reference counts for physical frames that are shared between
/// processes due to Copy-on-Write.
pub struct CowTracker {
    /// Reference counts for shared frames
    /// Key: frame number (physical address / FRAME_SIZE)
    /// Value: reference count (1 = exclusive, >1 = shared)
    counts: RwLock<BTreeMap<usize, u32>>,
}

impl CowTracker {
    /// Create a new COW tracker
    pub const fn new() -> Self {
        Self {
            counts: RwLock::new(BTreeMap::new()),
        }
    }

    /// Get the reference count for a frame
    ///
    /// Returns 0 if the frame is not tracked (not COW).
    pub fn ref_count(&self, phys: PhysAddr) -> u32 {
        let frame = phys.as_usize() / FRAME_SIZE;
        self.counts.read().get(&frame).copied().unwrap_or(0)
    }

    /// Increment reference count for a frame
    ///
    /// Called when sharing a frame (e.g., during fork).
    /// If the frame wasn't tracked, starts at count 2 (original + new reference).
    pub fn increment(&self, phys: PhysAddr) {
        let frame = phys.as_usize() / FRAME_SIZE;
        let mut counts = self.counts.write();
        let count = counts.entry(frame).or_insert(1);
        *count += 1;
    }

    /// Decrement reference count for a frame
    ///
    /// Called when a process no longer references a frame.
    /// Returns the new reference count.
    /// If count reaches 0, removes the entry.
    pub fn decrement(&self, phys: PhysAddr) -> u32 {
        let frame = phys.as_usize() / FRAME_SIZE;
        let mut counts = self.counts.write();

        if let Some(count) = counts.get_mut(&frame) {
            *count = count.saturating_sub(1);
            let new_count = *count;

            if new_count == 0 {
                counts.remove(&frame);
            }

            new_count
        } else {
            0
        }
    }

    /// Check if a frame is shared (COW)
    ///
    /// Returns true if reference count > 1.
    pub fn is_shared(&self, phys: PhysAddr) -> bool {
        self.ref_count(phys) > 1
    }

    /// Mark a frame as exclusively owned
    ///
    /// Sets reference count to 1, indicating single owner.
    pub fn set_exclusive(&self, phys: PhysAddr) {
        let frame = phys.as_usize() / FRAME_SIZE;
        let mut counts = self.counts.write();
        counts.insert(frame, 1);
    }

    /// Remove tracking for a frame
    ///
    /// Called when a frame is freed.
    pub fn remove(&self, phys: PhysAddr) {
        let frame = phys.as_usize() / FRAME_SIZE;
        self.counts.write().remove(&frame);
    }

    /// Increment reference counts for a range of frames
    pub fn increment_range(&self, start: PhysAddr, count: usize) {
        let start_frame = start.as_usize() / FRAME_SIZE;
        let mut counts = self.counts.write();

        for i in 0..count {
            let frame = start_frame + i;
            let c = counts.entry(frame).or_insert(1);
            *c += 1;
        }
    }

    /// Get number of tracked frames
    pub fn tracked_count(&self) -> usize {
        self.counts.read().len()
    }
}

/// Global COW tracker
static COW_TRACKER: CowTracker = CowTracker::new();

/// Get the global COW tracker
pub fn cow_tracker() -> &'static CowTracker {
    &COW_TRACKER
}

/// Increment reference count for a frame
pub fn cow_inc(phys: PhysAddr) {
    COW_TRACKER.increment(phys);
}

/// Decrement reference count for a frame
pub fn cow_dec(phys: PhysAddr) -> u32 {
    COW_TRACKER.decrement(phys)
}

/// Check if a frame is shared
pub fn cow_is_shared(phys: PhysAddr) -> bool {
    COW_TRACKER.is_shared(phys)
}

/// Get reference count for a frame
pub fn cow_count(phys: PhysAddr) -> u32 {
    COW_TRACKER.ref_count(phys)
}
