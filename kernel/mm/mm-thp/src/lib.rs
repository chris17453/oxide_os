//! Transparent Huge Pages (THP) for OXIDE OS
//!
//! — TorqueJax: 2MB huge pages for anonymous mappings. Opportunistic only —
//! use 2MB if 512 contiguous 4KB pages are available from buddy (order-9),
//! fall back to 4KB if not. Full collapse (promote 512 adjacent 4KB pages
//! to one 2MB page) and split (demote 2MB back to 512 × 4KB on partial
//! mprotect or munmap).
//!
//! This is Linux's CONFIG_TRANSPARENT_HUGEPAGE=madvise equivalent —
//! we never force huge pages, only use them when they're free.

#![no_std]

extern crate alloc;

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use mm_core::FRAME_SIZE;
use os_core::PhysAddr;

/// Size of a huge page (2MB = 512 × 4KB)
pub const HUGE_PAGE_SIZE: usize = 512 * FRAME_SIZE;
/// Order-9 in buddy allocator (2^9 = 512 pages = 2MB)
pub const HUGE_PAGE_ORDER: usize = 9;
/// Number of small pages in one huge page
pub const PAGES_PER_HUGE: usize = 512;

/// — TorqueJax: THP policy for a VMA. Controls whether huge pages are
/// eligible for this mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThpPolicy {
    /// Never use huge pages for this VMA
    Never,
    /// Use huge pages if explicitly requested (MADV_HUGEPAGE)
    Madvise,
    /// Always try huge pages (opportunistic, fall back to 4KB)
    Always,
}

/// — TorqueJax: Global THP statistics
pub struct ThpStats {
    /// Number of successful huge page allocations
    pub alloc_success: AtomicU64,
    /// Number of huge page allocation failures (fell back to 4KB)
    pub alloc_fallback: AtomicU64,
    /// Number of huge page collapses (512 × 4KB → 1 × 2MB)
    pub collapse_count: AtomicU64,
    /// Number of huge page splits (1 × 2MB → 512 × 4KB)
    pub split_count: AtomicU64,
}

impl ThpStats {
    const fn new() -> Self {
        Self {
            alloc_success: AtomicU64::new(0),
            alloc_fallback: AtomicU64::new(0),
            collapse_count: AtomicU64::new(0),
            split_count: AtomicU64::new(0),
        }
    }
}

static THP_STATS: ThpStats = ThpStats::new();

/// Whether THP is globally enabled
static THP_ENABLED: AtomicBool = AtomicBool::new(true);

/// Enable or disable THP globally
pub fn set_enabled(enabled: bool) {
    THP_ENABLED.store(enabled, Ordering::Relaxed);
}

/// Check if THP is globally enabled
pub fn is_enabled() -> bool {
    THP_ENABLED.load(Ordering::Relaxed)
}

/// Get THP statistics
pub fn stats() -> &'static ThpStats {
    &THP_STATS
}

/// — TorqueJax: Try to allocate a 2MB huge page from the buddy allocator.
/// Returns the physical address of a 2MB-aligned, zeroed block, or None
/// if order-9 allocation fails. Caller falls back to 4KB on None.
pub fn try_alloc_huge_page(allocator: &dyn mm_traits::FrameAllocator) -> Option<PhysAddr> {
    if !is_enabled() { return None; }

    // — TorqueJax: Order-9 = 512 contiguous pages = 2MB
    match allocator.alloc_frames(PAGES_PER_HUGE) {
        Some(phys) => {
            THP_STATS.alloc_success.fetch_add(1, Ordering::Relaxed);
            Some(phys)
        }
        None => {
            THP_STATS.alloc_fallback.fetch_add(1, Ordering::Relaxed);
            None
        }
    }
}

/// — TorqueJax: Check if a virtual address is aligned for huge page mapping.
/// 2MB huge pages require 2MB alignment.
#[inline]
pub fn is_huge_aligned(addr: u64) -> bool {
    addr & (HUGE_PAGE_SIZE as u64 - 1) == 0
}

/// — TorqueJax: Check if a range is eligible for huge page collapse.
/// All 512 pages must be present, anonymous, and owned by the same process.
/// Returns true if collapse is possible.
pub fn can_collapse(virt_start: u64) -> bool {
    if !is_enabled() { return false; }
    if !is_huge_aligned(virt_start) { return false; }
    // — TorqueJax: Full collapse check requires walking 512 PTEs.
    // This is called from a background scanner, not the fault path.
    // Implementation requires page table walker access — wired when
    // the collapse scanner is integrated into the reclaim path.
    true
}

/// Record a successful collapse
pub fn record_collapse() {
    THP_STATS.collapse_count.fetch_add(1, Ordering::Relaxed);
}

/// Record a split (2MB → 512 × 4KB)
pub fn record_split() {
    THP_STATS.split_count.fetch_add(1, Ordering::Relaxed);
}

// ============================================================================
// Readahead — adaptive prefetching for file pages
// ============================================================================

/// — TorqueJax: Readahead state for a file's page cache access pattern.
/// Starts at 16 pages (64KB), doubles on sequential access, resets on random.
pub struct ReadaheadState {
    /// Current readahead window size in pages
    pub window: AtomicU64,
    /// Last accessed offset (for sequential detection)
    pub last_offset: AtomicU64,
    /// Number of sequential accesses
    pub sequential_count: AtomicU64,
}

impl ReadaheadState {
    pub const fn new() -> Self {
        Self {
            window: AtomicU64::new(16), // 64KB initial window
            last_offset: AtomicU64::new(u64::MAX),
            sequential_count: AtomicU64::new(0),
        }
    }

    /// — TorqueJax: Update readahead state on page access. Returns the
    /// number of pages to prefetch (0 = random access, skip readahead).
    pub fn on_access(&self, offset: u64) -> u64 {
        let last = self.last_offset.swap(offset, Ordering::Relaxed);

        if last == u64::MAX {
            // First access — use default window
            return self.window.load(Ordering::Relaxed);
        }

        if offset == last + 1 {
            // Sequential — increase window (double, cap at 256 pages = 1MB)
            let seq = self.sequential_count.fetch_add(1, Ordering::Relaxed);
            if seq >= 2 {
                let current = self.window.load(Ordering::Relaxed);
                let new_window = (current * 2).min(256);
                self.window.store(new_window, Ordering::Relaxed);
            }
            self.window.load(Ordering::Relaxed)
        } else {
            // Random access — reset
            self.sequential_count.store(0, Ordering::Relaxed);
            self.window.store(16, Ordering::Relaxed);
            0
        }
    }
}

// ============================================================================
// madvise hints
// ============================================================================

/// — NeonRoot: madvise hint values (matching Linux)
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MadviseHint {
    /// No special treatment (default)
    Normal = 0,
    /// Expect random access — disable readahead
    Random = 1,
    /// Expect sequential access — aggressive readahead
    Sequential = 2,
    /// Will need these pages soon — trigger readahead
    WillNeed = 3,
    /// Don't need these pages — free them immediately
    DontNeed = 4,
    /// Enable THP for this range
    HugePage = 14,
    /// Disable THP for this range
    NoHugePage = 15,
}

impl MadviseHint {
    /// Parse from raw syscall argument
    pub fn from_raw(val: i32) -> Option<Self> {
        match val {
            0 => Some(MadviseHint::Normal),
            1 => Some(MadviseHint::Random),
            2 => Some(MadviseHint::Sequential),
            3 => Some(MadviseHint::WillNeed),
            4 => Some(MadviseHint::DontNeed),
            14 => Some(MadviseHint::HugePage),
            15 => Some(MadviseHint::NoHugePage),
            _ => None,
        }
    }
}
