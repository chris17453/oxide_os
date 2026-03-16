//! Swap Subsystem for OXIDE OS
//!
//! — IronGhost: When RAM is full and you've already evicted every clean page,
//! the only thing left to do is write anonymous pages to disk. Welcome to swap.
//! This module provides swap areas (block device backed), slot bitmaps for tracking
//! free/used slots, a swap cache for recently swapped-in pages, and PTE encoding
//! for non-present swap entries.
//!
//! SAFETY: Swap page-in MUST enable kernel preemption (kpo) during block I/O.
//! The fault handler normally runs with preemption context from the caller.
//! Block device reads may sleep. Follow docs/agents/write-syscall-kernel-preempt.md.

#![no_std]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use mm_vmstat::Counter as VmC;
use os_core::PhysAddr;
use spin::{Mutex, RwLock};

// ============================================================================
// Swap entry — encoded in non-present PTEs
// ============================================================================

/// — IronGhost: A swap entry identifies a page's location on a swap device.
/// Encoded into non-present PTEs: bit 0=0 (not present), bit 1=1 (swap marker),
/// bits [2:4]=area index, bits [5:36]=slot offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SwapEntry {
    /// Swap area index (0-7, 3 bits)
    pub area: u8,
    /// Slot offset within the swap area
    pub slot: u32,
}

impl SwapEntry {
    /// Encode a swap entry into a PTE-compatible u64
    /// Format: bit 0=0 (not present), bit 1=1 (swap marker),
    /// bits [2:4]=area, bits [5:36]=slot
    #[inline]
    pub fn encode(&self) -> u64 {
        let mut val: u64 = 0;
        val |= 1 << 1; // swap marker bit
        val |= ((self.area as u64) & 0x7) << 2;
        val |= (self.slot as u64) << 5;
        val
    }

    /// Decode a swap entry from a PTE value
    /// Returns None if the PTE is not a swap entry (bit 1 not set or bit 0 set)
    #[inline]
    pub fn decode(pte: u64) -> Option<Self> {
        // Must be: bit 0 = 0 (not present), bit 1 = 1 (swap marker)
        if pte & 1 != 0 { return None; } // present page, not swap
        if pte & 2 == 0 { return None; } // not a swap entry
        Some(SwapEntry {
            area: ((pte >> 2) & 0x7) as u8,
            slot: ((pte >> 5) & 0xFFFF_FFFF) as u32,
        })
    }

    /// Check if a PTE value is a swap entry
    #[inline]
    pub fn is_swap_entry(pte: u64) -> bool {
        pte & 1 == 0 && pte & 2 != 0
    }
}

// ============================================================================
// Swap area — a block device region used for paging
// ============================================================================

/// — IronGhost: A swap area backed by a block device. Each slot is one 4KB page.
/// The slot bitmap tracks which slots are in use. Allocation is first-fit scan.
pub struct SwapArea {
    /// Total number of 4KB slots in this area
    pub total_slots: u32,
    /// Bitmap of used slots (1 = in use, 0 = free)
    slot_map: Mutex<Vec<u64>>,
    /// Number of free slots
    pub free_count: AtomicU32,
    /// Whether this area is active
    pub active: AtomicBool,
}

impl SwapArea {
    /// Create a new swap area with the given number of slots
    pub fn new(total_slots: u32) -> Self {
        let bitmap_len = ((total_slots as usize) + 63) / 64;
        Self {
            total_slots,
            slot_map: Mutex::new(alloc::vec![0u64; bitmap_len]),
            free_count: AtomicU32::new(total_slots),
            active: AtomicBool::new(true),
        }
    }

    /// Allocate a free slot. Returns slot index or None if full.
    pub fn alloc_slot(&self) -> Option<u32> {
        if self.free_count.load(Ordering::Relaxed) == 0 {
            return None;
        }
        let mut bitmap = self.slot_map.lock();
        for (word_idx, word) in bitmap.iter_mut().enumerate() {
            if *word != u64::MAX {
                // — IronGhost: Find first zero bit
                let bit = (!*word).trailing_zeros();
                let slot = (word_idx as u32) * 64 + bit;
                if slot >= self.total_slots { return None; }
                *word |= 1u64 << bit;
                self.free_count.fetch_sub(1, Ordering::Relaxed);
                return Some(slot);
            }
        }
        None
    }

    /// Free a previously allocated slot
    pub fn free_slot(&self, slot: u32) {
        if slot >= self.total_slots { return; }
        let word_idx = (slot / 64) as usize;
        let bit = slot % 64;
        let mut bitmap = self.slot_map.lock();
        if word_idx < bitmap.len() {
            bitmap[word_idx] &= !(1u64 << bit);
            self.free_count.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Used slots count
    pub fn used_slots(&self) -> u32 {
        self.total_slots - self.free_count.load(Ordering::Relaxed)
    }
}

// ============================================================================
// Swap cache — recently swapped-in pages still in swap
// ============================================================================

/// — IronGhost: Swap cache maps SwapEntry → PhysAddr for recently swapped-in
/// pages. If a page is in the swap cache, we don't need to read from disk again.
/// The swap slot is only freed when the page is no longer in the cache.
pub struct SwapCache {
    cache: RwLock<BTreeMap<SwapEntry, PhysAddr>>,
}

impl SwapCache {
    const fn new() -> Self {
        Self {
            cache: RwLock::new(BTreeMap::new()),
        }
    }

    /// Look up a swap entry in the cache
    pub fn find(&self, entry: &SwapEntry) -> Option<PhysAddr> {
        let cache = self.cache.read();
        cache.get(entry).copied()
    }

    /// Add a page to the swap cache
    pub fn insert(&self, entry: SwapEntry, phys: PhysAddr) {
        let mut cache = self.cache.write();
        cache.insert(entry, phys);
    }

    /// Remove a page from the swap cache (e.g., after freeing the swap slot)
    pub fn remove(&self, entry: &SwapEntry) -> Option<PhysAddr> {
        let mut cache = self.cache.write();
        cache.remove(entry)
    }
}

// ============================================================================
// Swap subsystem — global state
// ============================================================================

/// — IronGhost: The global swap subsystem. Manages swap areas, the swap cache,
/// and global swap statistics.
pub struct SwapSubsystem {
    /// Registered swap areas (max 8)
    pub areas: RwLock<Vec<SwapArea>>,
    /// Swap cache for recently paged-in pages
    pub cache: SwapCache,
    /// Total swap pages across all areas
    pub total_pages: AtomicU64,
    /// Currently used swap pages
    pub used_pages: AtomicU64,
    /// Swappiness — controls anon vs file reclaim ratio (0-200, default 60)
    pub swappiness: AtomicU64,
}

impl SwapSubsystem {
    const fn new() -> Self {
        Self {
            areas: RwLock::new(Vec::new()),
            cache: SwapCache::new(),
            total_pages: AtomicU64::new(0),
            used_pages: AtomicU64::new(0),
            swappiness: AtomicU64::new(60),
        }
    }

    /// Add a swap area with the given number of slots
    pub fn add_area(&self, total_slots: u32) {
        let mut areas = self.areas.write();
        if areas.len() >= 8 { return; } // Max 8 swap areas
        areas.push(SwapArea::new(total_slots));
        self.total_pages.fetch_add(total_slots as u64, Ordering::Relaxed);
    }

    /// Allocate a swap slot from the first available area.
    /// Returns (area_index, slot) or None if all areas are full.
    pub fn alloc_slot(&self) -> Option<SwapEntry> {
        let areas = self.areas.read();
        for (i, area) in areas.iter().enumerate() {
            if !area.active.load(Ordering::Relaxed) { continue; }
            if let Some(slot) = area.alloc_slot() {
                self.used_pages.fetch_add(1, Ordering::Relaxed);
                return Some(SwapEntry { area: i as u8, slot });
            }
        }
        None
    }

    /// Free a swap slot
    pub fn free_slot(&self, entry: &SwapEntry) {
        let areas = self.areas.read();
        if (entry.area as usize) < areas.len() {
            areas[entry.area as usize].free_slot(entry.slot);
            self.used_pages.fetch_sub(1, Ordering::Relaxed);
        }
        // — IronGhost: Also remove from swap cache if present
        self.cache.remove(entry);
    }

    /// Total swap space in pages
    pub fn total(&self) -> u64 {
        self.total_pages.load(Ordering::Relaxed)
    }

    /// Used swap space in pages
    pub fn used(&self) -> u64 {
        self.used_pages.load(Ordering::Relaxed)
    }

    /// Free swap space in pages
    pub fn free(&self) -> u64 {
        self.total().saturating_sub(self.used())
    }

    /// Current swappiness setting
    pub fn get_swappiness(&self) -> u64 {
        self.swappiness.load(Ordering::Relaxed)
    }

    /// Set swappiness (0 = file-only reclaim, 200 = aggressive anon swap)
    pub fn set_swappiness(&self, val: u64) {
        self.swappiness.store(val.min(200), Ordering::Relaxed);
    }
}

/// — IronGhost: Global swap subsystem singleton
static SWAP_SUBSYSTEM: SwapSubsystem = SwapSubsystem::new();

/// Get a reference to the global swap subsystem
pub fn swap() -> &'static SwapSubsystem {
    &SWAP_SUBSYSTEM
}

// ============================================================================
// Page-out pipeline (reclaim scanner → swap)
// ============================================================================

/// — IronGhost: Write a page out to swap. Called from reclaim scanner for
/// anonymous pages that need to be evicted.
///
/// Steps:
/// 1. Allocate swap slot
/// 2. Write page to block device at slot offset
/// 3. Replace PTE with swap entry encoding
/// 4. Remove from LRU, free frame to buddy
/// 5. Increment vmstat::PgSwapOut
///
/// Returns the swap entry on success.
pub fn page_out(phys: PhysAddr) -> Option<SwapEntry> {
    // Step 1: Allocate swap slot
    let entry = SWAP_SUBSYSTEM.alloc_slot()?;

    // Step 2: Write page data to swap device
    // — IronGhost: TODO when block device integration ships.
    // For now, the swap cache holds the mapping, so page-in can find it.
    SWAP_SUBSYSTEM.cache.insert(entry, phys);

    // Step 5: Count it
    mm_vmstat::inc(VmC::PgSwapOut);

    Some(entry)
}

/// — IronGhost: Read a page back from swap. Called from page fault handler
/// when a non-present PTE has the swap marker bit set.
///
/// Steps:
/// 1. Check swap cache (hit → reuse frame, no I/O)
/// 2. Miss → alloc frame, read from block device
/// 3. Free swap slot, add to LRU
/// 4. Increment vmstat::PgSwapIn
///
/// Returns the physical frame on success.
pub fn page_in(entry: &SwapEntry) -> Option<PhysAddr> {
    // Step 1: Check swap cache
    if let Some(phys) = SWAP_SUBSYSTEM.cache.find(entry) {
        // — IronGhost: Cache hit — no I/O needed. Remove from cache,
        // free the swap slot, count it, return the frame.
        SWAP_SUBSYSTEM.cache.remove(entry);
        SWAP_SUBSYSTEM.free_slot(entry);
        mm_vmstat::inc(VmC::PgSwapIn);
        return Some(phys);
    }

    // Step 2: Cache miss — need to read from block device
    // — IronGhost: Block device I/O integration goes here.
    // This requires kpo (kernel preempt ok) to be enabled by the caller.
    // For now, return None to indicate swap-in failed (page stays swapped).
    None
}

/// — IronGhost: Free all swap slots owned by a process's address space.
/// Called from UserAddressSpace::Drop to prevent swap slot leaks on exit.
/// Scans PTEs for swap entries and frees their slots.
pub fn free_swap_entries(entries: &[SwapEntry]) {
    for entry in entries {
        SWAP_SUBSYSTEM.free_slot(entry);
    }
}
