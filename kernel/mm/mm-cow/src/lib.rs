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

        // — SableWire: Mirror refcount to page frame database
        if let Some(db) = mm_pagedb::try_pagedb() {
            db.ref_inc(phys);
            if let Some(pf) = db.get(phys) {
                pf.set_flag(mm_pagedb::PF_COW);
            }
        }
    }

    /// Decrement reference count for a frame
    ///
    /// Called when a process no longer references a frame.
    /// Returns the new reference count.
    /// If count reaches 0, removes the entry.
    ///
    /// — ColdCipher: Stale entry guard — if the pagedb shows the frame as
    /// already FREE, the BTreeMap entry is a ghost. Remove it without calling
    /// ref_dec (which would underflow and cascade into DoubleFree). This happens
    /// when a frame is freed through a non-COW path (munmap, exec cleanup) but
    /// the BTreeMap wasn't updated.
    pub fn decrement(&self, phys: PhysAddr) -> u32 {
        let frame = phys.as_usize() / FRAME_SIZE;
        let mut counts = self.counts.write();

        if let Some(count) = counts.get_mut(&frame) {
            *count = count.saturating_sub(1);
            let new_count = *count;

            if new_count == 0 {
                counts.remove(&frame);
            }

            // — SableWire: Mirror decrement to page frame database.
            // But first check if the frame is still alive and still a data page.
            // Guard 1: if already freed (PF_FREE, rc=0), skip ref_dec to prevent underflow.
            // Guard 2: if recycled as a PT structure (PF_PAGETABLE), this BTreeMap
            // entry is stale — the frame was freed, returned to buddy, and re-allocated
            // as a page table by a later fork. PT frames are NEVER COW-shared.
            // Decrementing their refcount cascades into RefcountUnderflow and then
            // DoubleFree when their actual owner's Drop tries to free them.
            if let Some(db) = mm_pagedb::try_pagedb() {
                if let Some(pf) = db.get(phys) {
                    let flags = pf.flags();
                    if flags == mm_pagedb::PF_FREE && pf.refcount() == 0 {
                        // — ColdCipher: Stale entry. Frame was freed elsewhere.
                        // BTreeMap is cleaned up above. Don't touch pagedb.
                        return new_count;
                    }
                    if flags & mm_pagedb::PF_PAGETABLE != 0 {
                        // — WireSaint: Frame was recycled as a PT structure.
                        // This BTreeMap entry is a ghost from when the frame was
                        // a user data page. Purge it silently — the PT frame's
                        // owner manages its refcount through mark_pagetable/mark_free.
                        return new_count;
                    }
                }
                db.ref_dec(phys);
                if new_count <= 1 {
                    if let Some(pf) = db.get(phys) {
                        pf.clear_flag(mm_pagedb::PF_COW);
                    }
                }
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

    /// Atomically claim exclusive ownership of a frame.
    ///
    /// — ColdCipher: The old ref_count() + remove()/decrement() dance was a
    /// TOCTOU time bomb. Between releasing the read lock and acquiring the write
    /// lock, a concurrent fork() can increment the count. Result: two processes
    /// both think they own the frame exclusively, both make it writable, both
    /// write to the same physical memory. Silent corruption, zero signals.
    ///
    /// This method holds a SINGLE write lock for the entire check-and-act:
    ///   - count was 0 (not tracked): exclusive owner, returns true
    ///   - count was 1: last sharer, removes entry, returns true
    ///   - count was >1: still shared, decrements, returns false (caller must copy)
    ///
    /// If this returns true, the caller can make the page writable in-place.
    /// If false, the caller must allocate a new frame, copy, and remap.
    pub fn try_claim_exclusive(&self, phys: PhysAddr) -> bool {
        let frame = phys.as_usize() / FRAME_SIZE;
        let mut counts = self.counts.write();

        match counts.get_mut(&frame) {
            Some(count) if *count <= 1 => {
                // — ColdCipher: We're the last one holding this frame. Remove
                // the tracker entry entirely — no one else has a reference.
                counts.remove(&frame);

                // — SableWire: Claiming exclusive ownership — do NOT ref_dec.
                // The frame transitions from COW-shared to single-owner. The
                // pagedb refcount should stay at 1 (one live owner). If we
                // decremented here, rc would drop to 0 while the frame is
                // still mapped and in use. Then a subsequent fork() would
                // ref_inc from 0→1 while BTreeMap goes 0→2, creating a
                // desync that cascades into RefcountUnderflow on every
                // downstream decrement. Just clear the COW flag.
                if let Some(db) = mm_pagedb::try_pagedb() {
                    if let Some(pf) = db.get(phys) {
                        pf.clear_flag(mm_pagedb::PF_COW);
                    }
                }

                true
            }
            Some(count) => {
                // — ColdCipher: Still shared. Decrement our reference and tell
                // the caller to copy. The frame stays tracked for other owners.
                *count -= 1;

                // — SableWire: Mirror decrement to pagedb. Guard against stale
                // and recycled entries — same logic as decrement() above.
                if let Some(db) = mm_pagedb::try_pagedb() {
                    let skip = db.get(phys).map_or(false, |pf| {
                        let flags = pf.flags();
                        // — ColdCipher: Already freed — stale BTreeMap entry.
                        (flags == mm_pagedb::PF_FREE && pf.refcount() == 0)
                        // — WireSaint: Recycled as PT structure — not COW anymore.
                        || (flags & mm_pagedb::PF_PAGETABLE != 0)
                    });
                    if !skip {
                        db.ref_dec(phys);
                    }
                    if *count <= 1 {
                        if let Some(pf) = db.get(phys) {
                            pf.clear_flag(mm_pagedb::PF_COW);
                        }
                    }
                }

                false
            }
            None => {
                // — ColdCipher: Not tracked = never was COW, or already removed.
                // Either way, we own it exclusively.
                true
            }
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
