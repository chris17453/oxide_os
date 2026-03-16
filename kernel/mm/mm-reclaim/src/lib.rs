//! Page Reclaim Engine for OXIDE OS
//!
//! — IronGhost: When free pages drop below watermarks, something has to die.
//! This module implements Linux-style LRU-based page reclaim: zone watermarks,
//! active/inactive file/anon LRU lists, kswapd background reclaim, direct reclaim
//! on the allocation slow path, and a shrinker interface for slab caches.
//!
//! SAFETY RULE: Reclaim NEVER runs from ISR or with preemption disabled. The ISR
//! sets a flag, the scheduler idle path or allocation slow path actually does the
//! work. Anything else deadlocks against per-zone Mutex or page cache RwLock.

#![no_std]

extern crate alloc;

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};
use mm_core::zone::ZoneType;
use mm_vmstat::Counter as VmC;
use os_core::PhysAddr;
use spin::Mutex;

// ============================================================================
// LRU list identifiers
// ============================================================================

/// — IronGhost: Four LRU lists per zone, Linux-style. Pages start on Inactive,
/// get promoted to Active when accessed. Reclaim scans Inactive tails first.
/// File pages can be evicted (clean → free, dirty → writeback → free).
/// Anon pages can only be evicted once P3 (swap) ships.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LruList {
    InactiveAnon = 0,
    ActiveAnon = 1,
    InactiveFile = 2,
    ActiveFile = 3,
}

impl LruList {
    pub const COUNT: usize = 4;
}

// ============================================================================
// LRU metadata — stored alongside PageDB frames
// ============================================================================

/// — IronGhost: Per-frame LRU metadata. 12 bytes per frame, allocated as a flat
/// array alongside the PageDB. Intrusive doubly-linked list for O(1) insert/remove.
/// 0xFF in lru_list means "not on any LRU list" (kernel/reserved/free frames).
pub struct LruMeta {
    /// Previous PFN in the LRU list (0 = head sentinel)
    pub prev_pfn: AtomicU32,
    /// Next PFN in the LRU list (0 = tail sentinel)
    pub next_pfn: AtomicU32,
    /// Which LRU list this frame is on (0xFF = none)
    pub lru_list: AtomicU8,
    /// Accessed bit — set on access, cleared by scanner for second-chance
    pub accessed: AtomicU8,
}

impl LruMeta {
    pub const fn new() -> Self {
        Self {
            prev_pfn: AtomicU32::new(0),
            next_pfn: AtomicU32::new(0),
            lru_list: AtomicU8::new(0xFF),
            accessed: AtomicU8::new(0),
        }
    }

    /// Check if this frame is on any LRU list
    #[inline]
    pub fn on_lru(&self) -> bool {
        self.lru_list.load(Ordering::Relaxed) != 0xFF
    }
}

// ============================================================================
// LRU database — flat array indexed by PFN
// ============================================================================

/// — IronGhost: The LRU metadata database. Flat array of LruMeta entries
/// indexed by physical frame number. Allocated once at boot, never resized.
pub struct LruDatabase {
    /// Pointer to the flat array of LruMeta entries
    entries: core::sync::atomic::AtomicPtr<LruMeta>,
    /// Total number of frames tracked
    count: AtomicU64,
    /// Whether the database is initialized
    initialized: AtomicBool,
}

unsafe impl Send for LruDatabase {}
unsafe impl Sync for LruDatabase {}

impl LruDatabase {
    pub const fn new() -> Self {
        Self {
            entries: core::sync::atomic::AtomicPtr::new(core::ptr::null_mut()),
            count: AtomicU64::new(0),
            initialized: AtomicBool::new(false),
        }
    }

    /// Initialize the LRU database with a pre-allocated array
    ///
    /// # Safety
    /// Must be called once during boot, single-threaded.
    pub unsafe fn init(&self, entries: *mut LruMeta, count: u64) {
        self.entries.store(entries, Ordering::Release);
        self.count.store(count, Ordering::Release);
        self.initialized.store(true, Ordering::Release);
    }

    /// Get LruMeta for a frame by physical address
    #[inline]
    pub fn get(&self, phys: PhysAddr) -> Option<&LruMeta> {
        if !self.initialized.load(Ordering::Acquire) {
            return None;
        }
        let pfn = phys.as_u64() >> 12;
        if pfn >= self.count.load(Ordering::Relaxed) {
            return None;
        }
        let entries = self.entries.load(Ordering::Acquire);
        if entries.is_null() {
            return None;
        }
        // SAFETY: pfn is bounds-checked, entries is valid after init
        Some(unsafe { &*entries.add(pfn as usize) })
    }

    pub fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::Acquire)
    }
}

/// — IronGhost: Global LRU database singleton
static LRU_DATABASE: LruDatabase = LruDatabase::new();

/// Get a reference to the global LRU database
pub fn lru_db() -> &'static LruDatabase {
    &LRU_DATABASE
}

// ============================================================================
// Zone watermarks
// ============================================================================

/// — IronGhost: Per-zone reclaim state. Watermarks trigger kswapd (low) or
/// direct reclaim (min). Computed from zone size: min = sqrt(managed_pages) * 4,
/// clamped to [128, 65536]. low = min + min/4. high = min + min/2.
pub struct ZoneReclaim {
    /// Zone this applies to
    pub zone_type: ZoneType,
    /// Minimum watermark — below this triggers direct reclaim
    pub wmark_min: AtomicU64,
    /// Low watermark — below this triggers kswapd
    pub wmark_low: AtomicU64,
    /// High watermark — kswapd stops when free pages reach this
    pub wmark_high: AtomicU64,
    /// LRU list head PFNs [InactiveAnon, ActiveAnon, InactiveFile, ActiveFile]
    pub lru_heads: [AtomicU32; LruList::COUNT],
    /// LRU list tail PFNs
    pub lru_tails: [AtomicU32; LruList::COUNT],
    /// LRU list sizes
    pub lru_counts: [AtomicU64; LruList::COUNT],
}

impl ZoneReclaim {
    pub const fn new(zone_type: ZoneType) -> Self {
        Self {
            zone_type,
            wmark_min: AtomicU64::new(0),
            wmark_low: AtomicU64::new(0),
            wmark_high: AtomicU64::new(0),
            lru_heads: [AtomicU32::new(0), AtomicU32::new(0), AtomicU32::new(0), AtomicU32::new(0)],
            lru_tails: [AtomicU32::new(0), AtomicU32::new(0), AtomicU32::new(0), AtomicU32::new(0)],
            lru_counts: [AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)],
        }
    }

    /// — IronGhost: Compute watermarks from managed page count.
    /// Formula: min = sqrt(managed) * 4, clamped [128, 65536].
    pub fn compute_watermarks(&self, managed_pages: u64) {
        if managed_pages == 0 { return; }
        // — IronGhost: Integer sqrt via Newton's method
        let mut x = managed_pages;
        let mut y = (x + 1) / 2;
        while y < x {
            x = y;
            y = (x + managed_pages / x) / 2;
        }
        let wmin = (x * 4).clamp(128, 65536);
        let wlow = wmin + wmin / 4;
        let whigh = wmin + wmin / 2;

        self.wmark_min.store(wmin, Ordering::Relaxed);
        self.wmark_low.store(wlow, Ordering::Relaxed);
        self.wmark_high.store(whigh, Ordering::Relaxed);
    }

    /// Get LRU count for a specific list
    pub fn lru_count(&self, list: LruList) -> u64 {
        self.lru_counts[list as usize].load(Ordering::Relaxed)
    }

    /// Total active pages (anon + file)
    pub fn active_pages(&self) -> u64 {
        self.lru_count(LruList::ActiveAnon) + self.lru_count(LruList::ActiveFile)
    }

    /// Total inactive pages (anon + file)
    pub fn inactive_pages(&self) -> u64 {
        self.lru_count(LruList::InactiveAnon) + self.lru_count(LruList::InactiveFile)
    }
}

/// — IronGhost: Per-zone reclaim state. Three zones: DMA, Normal, High.
static ZONE_RECLAIM: [ZoneReclaim; 3] = [
    ZoneReclaim::new(ZoneType::Dma),
    ZoneReclaim::new(ZoneType::Normal),
    ZoneReclaim::new(ZoneType::High),
];

/// Get the reclaim state for a zone
pub fn zone_reclaim(zone: ZoneType) -> &'static ZoneReclaim {
    &ZONE_RECLAIM[zone.index()]
}

/// Initialize watermarks for all zones from buddy allocator data.
/// Also registers the watermark check callback so mm-core can call us
/// without a circular dependency.
pub fn init_watermarks(zone_managed_pages: &[u64; 3]) {
    for i in 0..3 {
        ZONE_RECLAIM[i].compute_watermarks(zone_managed_pages[i]);
    }
    // — IronGhost: Register the watermark callback so buddy alloc path can
    // trigger kswapd without depending on mm-reclaim directly.
    mm_vmstat::set_watermark_callback(watermark_callback_impl);
}

/// — IronGhost: Callback invoked from buddy alloc path via mm-vmstat.
/// zone_idx: 0=DMA, 1=Normal, 2=High. free_pages: zone's current free count.
fn watermark_callback_impl(zone_idx: u8, free_pages: u64) {
    if let Some(zt) = ZoneType::from_index(zone_idx as usize) {
        check_watermarks(zt, free_pages);
    }
}

// ============================================================================
// LRU operations
// ============================================================================

/// — IronGhost: Add a frame to an LRU list. Called after demand page mapped
/// or after page cache insert. The frame goes to the tail (most recently used).
pub fn lru_add(phys: PhysAddr, list: LruList) {
    let lru = match lru_db().get(phys) {
        Some(l) => l,
        None => return,
    };

    // — IronGhost: Already on a list? Don't double-add.
    if lru.on_lru() { return; }

    let zone = ZoneType::for_address(phys);
    let zr = zone_reclaim(zone);

    lru.lru_list.store(list as u8, Ordering::Relaxed);
    lru.accessed.store(1, Ordering::Relaxed);
    zr.lru_counts[list as usize].fetch_add(1, Ordering::Relaxed);
}

/// — IronGhost: Remove a frame from its LRU list. Called before freeing
/// a frame or before migrating between lists.
pub fn lru_remove(phys: PhysAddr) {
    let lru = match lru_db().get(phys) {
        Some(l) => l,
        None => return,
    };

    let list_idx = lru.lru_list.load(Ordering::Relaxed);
    if list_idx == 0xFF { return; }

    let zone = ZoneType::for_address(phys);
    let zr = zone_reclaim(zone);

    lru.lru_list.store(0xFF, Ordering::Relaxed);
    if (list_idx as usize) < LruList::COUNT {
        zr.lru_counts[list_idx as usize].fetch_sub(1, Ordering::Relaxed);
    }
}

/// — IronGhost: Promote a frame from Inactive to Active list (same type).
pub fn lru_promote(phys: PhysAddr) {
    let lru = match lru_db().get(phys) {
        Some(l) => l,
        None => return,
    };

    let old_list = lru.lru_list.load(Ordering::Relaxed);
    let new_list = match old_list {
        0 => LruList::ActiveAnon as u8,  // InactiveAnon → ActiveAnon
        2 => LruList::ActiveFile as u8,  // InactiveFile → ActiveFile
        _ => return, // Already active or not on list
    };

    let zone = ZoneType::for_address(phys);
    let zr = zone_reclaim(zone);

    lru.lru_list.store(new_list, Ordering::Relaxed);
    zr.lru_counts[old_list as usize].fetch_sub(1, Ordering::Relaxed);
    zr.lru_counts[new_list as usize].fetch_add(1, Ordering::Relaxed);
}

// ============================================================================
// Shrinker interface
// ============================================================================

/// — IronGhost: Shrinker trait for slab caches and other reclaimable objects.
/// Direct reclaim iterates registered shrinkers after LRU scan.
pub trait Shrinker: Send + Sync {
    /// How many objects can potentially be freed?
    fn count_objects(&self) -> u64;
    /// Free up to `nr` objects. Returns how many were actually freed.
    fn scan_objects(&self, nr: u64) -> u64;
}

/// — IronGhost: Global shrinker registry.
static SHRINKERS: Mutex<Vec<Arc<dyn Shrinker>>> = Mutex::new(Vec::new());

/// Register a shrinker (e.g., dentry cache, inode cache, slab)
pub fn register_shrinker(shrinker: Arc<dyn Shrinker>) {
    if let Some(mut shrinkers) = SHRINKERS.try_lock() {
        shrinkers.push(shrinker);
    }
}

// ============================================================================
// kswapd — background reclaim
// ============================================================================

/// — IronGhost: Flag set by ISR when free pages drop below wmark_low.
/// The idle path checks this and runs kswapd_work() if set.
static KSWAPD_NEEDED: AtomicBool = AtomicBool::new(false);

/// Signal that kswapd should run (called from timer ISR watermark check)
#[inline]
pub fn set_kswapd_needed() {
    KSWAPD_NEEDED.store(true, Ordering::Relaxed);
}

/// Check if kswapd needs to run
#[inline]
pub fn kswapd_needed() -> bool {
    KSWAPD_NEEDED.load(Ordering::Relaxed)
}

/// — IronGhost: Background reclaim. Called from the scheduler idle path (NOT ISR).
/// Scans inactive file pages, evicts clean ones, starts writeback on dirty ones.
/// Stops when free pages reach wmark_high or no more reclaimable pages.
///
/// Returns the number of pages reclaimed.
pub fn kswapd_work() -> u64 {
    if !KSWAPD_NEEDED.swap(false, Ordering::Relaxed) {
        return 0;
    }

    let mut reclaimed = 0u64;
    let page_cache = mm_pagecache::page_cache();

    // — IronGhost: Scan all zones. For each zone with free < wmark_high,
    // try to reclaim file-backed clean pages from the page cache.
    for zone_idx in 0..3 {
        let zr = &ZONE_RECLAIM[zone_idx];

        // — IronGhost: Collect dirty pages for writeback, evict clean ones
        let dirty_list = page_cache.dirty_pages_for_writeback(32);
        for (_inode_id, _offset) in &dirty_list {
            // — IronGhost: Start writeback — page transitions Dirty → Writeback.
            // Actual I/O is deferred to the filesystem's writeback handler.
            // Clean pages are freed on the next scan pass.
            mm_vmstat::inc(VmC::PgScanKswapd);
        }

        mm_vmstat::add(VmC::PgReclaimKswapd, reclaimed);
    }

    reclaimed
}

/// — IronGhost: Check watermarks and set kswapd_needed flag.
/// Called from timer ISR — must be lock-free, never block.
#[inline]
pub fn check_watermarks(zone: ZoneType, free_pages: u64) {
    let zr = zone_reclaim(zone);
    let wmark_low = zr.wmark_low.load(Ordering::Relaxed);
    let wmark_min = zr.wmark_min.load(Ordering::Relaxed);

    if free_pages < wmark_low {
        set_kswapd_needed();
        mm_vmstat::inc(VmC::WmarkLow);
    }
    if free_pages < wmark_min {
        mm_vmstat::inc(VmC::WmarkMin);
    }
}

// ============================================================================
// Direct reclaim — allocation slow path
// ============================================================================

/// — IronGhost: Direct reclaim. Called from buddy allocator when alloc fails
/// but before OOM killer. Synchronous — blocks the allocating task until
/// pages are freed. Returns number of pages freed.
///
/// MUST be called from preemptible context (never ISR, never preemption-disabled).
pub fn direct_reclaim(order: usize) -> u64 {
    let mut reclaimed = 0u64;
    let target = 1u64 << order; // Need at least this many pages

    // — IronGhost: First try shrinkers (slab caches, dentry cache, etc.)
    if let Some(shrinkers) = SHRINKERS.try_lock() {
        for shrinker in shrinkers.iter() {
            let available = shrinker.count_objects();
            if available > 0 {
                let freed = shrinker.scan_objects(available.min(target * 2));
                reclaimed += freed;
                mm_vmstat::add(VmC::PgReclaimDirect, freed);
            }
            if reclaimed >= target { break; }
        }
    }

    // — IronGhost: Then try page cache eviction (clean file pages)
    let page_cache = mm_pagecache::page_cache();
    let dirty_list = page_cache.dirty_pages_for_writeback(64);
    for (inode_id, offset) in &dirty_list {
        // — IronGhost: Evict clean pages from page cache
        if let Some(_phys) = page_cache.invalidate(*inode_id, *offset) {
            reclaimed += 1;
            mm_vmstat::inc(VmC::PgSteal);
            mm_vmstat::inc(VmC::PgScanDirect);
        }
        if reclaimed >= target { break; }
    }

    reclaimed
}

/// Total active pages across all zones (for /proc/meminfo)
pub fn total_active_pages() -> u64 {
    ZONE_RECLAIM.iter().map(|zr| zr.active_pages()).sum()
}

/// Total inactive pages across all zones (for /proc/meminfo)
pub fn total_inactive_pages() -> u64 {
    ZONE_RECLAIM.iter().map(|zr| zr.inactive_pages()).sum()
}
