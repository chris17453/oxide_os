//! Unified Page Cache for OXIDE OS
//!
//! — SableWire: Maps (inode_id, page_offset) → physical frame with dirty/clean/writeback
//! state tracking. Every file read goes through here first — cache hit means zero disk I/O.
//! Dirty pages queue for background writeback. This is the Linux page cache model: the
//! buffer cache and page cache unified into one structure. No separate buffer heads, no
//! block-level caching, just pages indexed by (inode, offset). Simple, fast, correct.
//!
//! SAFETY INVARIANT: The page cache is for disk-backed files ONLY. Heap/slab/buddy
//! allocators must NEVER go through the page cache — BTreeMap insertion allocates from
//! the heap, which would cause infinite recursion. — SableWire

#![no_std]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicU32, AtomicU64, AtomicU8, Ordering};
use mm_vmstat::Counter as VmC;
use os_core::PhysAddr;
use spin::RwLock;

/// — SableWire: Inode identifier. Opaque u64 — filesystems assign these.
/// Zero means "no inode" (anonymous mappings don't go through page cache).
pub type InodeId = u64;

/// — SableWire: Page lifecycle states. Transitions are strictly ordered:
/// Clean → Dirty → Writeback → Clean. Locked is transient during I/O setup.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageState {
    /// Page matches on-disk content (or was just read from disk)
    Clean = 0,
    /// Page was modified — needs writeback before eviction
    Dirty = 1,
    /// Writeback in progress — I/O submitted, waiting for completion
    Writeback = 2,
    /// Page locked for I/O setup (transient, held briefly)
    Locked = 3,
}

/// — SableWire: A single cached page. Tracks physical frame, state, refcount,
/// and access/dirty timestamps for LRU and writeback ordering.
pub struct CachedPage {
    /// Physical frame backing this page
    pub phys: PhysAddr,
    /// Current page state (Clean/Dirty/Writeback/Locked)
    pub state: AtomicU8,
    /// Reference count — number of active users (mmap, read buffer, etc.)
    pub refcount: AtomicU32,
    /// Tick when page was last accessed (for LRU promotion)
    pub access_tick: AtomicU64,
    /// Tick when page was marked dirty (for age-ordered writeback)
    pub dirty_tick: AtomicU64,
}

impl CachedPage {
    /// Create a new cached page in Clean state
    fn new(phys: PhysAddr) -> Self {
        Self {
            phys,
            state: AtomicU8::new(PageState::Clean as u8),
            refcount: AtomicU32::new(1),
            access_tick: AtomicU64::new(0),
            dirty_tick: AtomicU64::new(0),
        }
    }

    /// Get current state
    #[inline]
    pub fn get_state(&self) -> PageState {
        match self.state.load(Ordering::Acquire) {
            0 => PageState::Clean,
            1 => PageState::Dirty,
            2 => PageState::Writeback,
            _ => PageState::Locked,
        }
    }

    /// Check if page is dirty (needs writeback)
    #[inline]
    pub fn is_dirty(&self) -> bool {
        self.state.load(Ordering::Relaxed) == PageState::Dirty as u8
    }
}

/// — SableWire: Per-inode address space. Maps page offsets to cached frames.
/// This is Linux's `struct address_space` — one per inode, shared across all
/// open file descriptors pointing at the same file.
pub struct AddressSpace {
    /// Inode this address space belongs to
    pub inode_id: InodeId,
    /// Page index: offset (page-aligned >> 12) → cached page
    pages: RwLock<BTreeMap<u64, CachedPage>>,
    /// Number of dirty pages in this address space
    pub dirty_count: AtomicU64,
    /// Total cached pages in this address space
    pub cached_count: AtomicU64,
}

impl AddressSpace {
    /// Create a new empty address space for an inode
    fn new(inode_id: InodeId) -> Self {
        Self {
            inode_id,
            pages: RwLock::new(BTreeMap::new()),
            dirty_count: AtomicU64::new(0),
            cached_count: AtomicU64::new(0),
        }
    }

    /// Find a cached page at the given offset (page-aligned >> 12)
    pub fn find_page(&self, offset: u64) -> Option<PhysAddr> {
        let pages = self.pages.read();
        pages.get(&offset).map(|cp| {
            cp.access_tick.store(get_tick(), Ordering::Relaxed);
            cp.phys
        })
    }

    /// Insert a page into the cache. Returns previous frame if one existed.
    pub fn insert_page(&self, offset: u64, phys: PhysAddr) -> Option<PhysAddr> {
        let mut pages = self.pages.write();
        let old = pages.insert(offset, CachedPage::new(phys)).map(|cp| cp.phys);
        if old.is_none() {
            self.cached_count.fetch_add(1, Ordering::Relaxed);
        }
        old
    }

    /// Mark a page dirty at the given offset. Returns false if page not found.
    pub fn mark_dirty(&self, offset: u64) -> bool {
        let pages = self.pages.read();
        if let Some(cp) = pages.get(&offset) {
            let old = cp.state.swap(PageState::Dirty as u8, Ordering::AcqRel);
            if old != PageState::Dirty as u8 {
                self.dirty_count.fetch_add(1, Ordering::Relaxed);
                cp.dirty_tick.store(get_tick(), Ordering::Relaxed);
                mm_vmstat::inc(VmC::PgDirty);
            }
            true
        } else {
            false
        }
    }

    /// Start writeback on a dirty page. Transitions Dirty → Writeback.
    /// Returns the physical address if transition succeeded.
    pub fn start_writeback(&self, offset: u64) -> Option<PhysAddr> {
        let pages = self.pages.read();
        if let Some(cp) = pages.get(&offset) {
            let old = cp.state.compare_exchange(
                PageState::Dirty as u8,
                PageState::Writeback as u8,
                Ordering::AcqRel,
                Ordering::Relaxed,
            );
            if old.is_ok() {
                self.dirty_count.fetch_sub(1, Ordering::Relaxed);
                mm_vmstat::inc(VmC::PgWriteback);
                return Some(cp.phys);
            }
        }
        None
    }

    /// Complete writeback on a page. Transitions Writeback → Clean.
    pub fn end_writeback(&self, offset: u64) {
        let pages = self.pages.read();
        if let Some(cp) = pages.get(&offset) {
            let _ = cp.state.compare_exchange(
                PageState::Writeback as u8,
                PageState::Clean as u8,
                Ordering::AcqRel,
                Ordering::Relaxed,
            );
            mm_vmstat::inc(VmC::PgWritten);
        }
    }

    /// Invalidate (remove) a cached page. Returns the physical frame if removed.
    pub fn invalidate(&self, offset: u64) -> Option<PhysAddr> {
        let mut pages = self.pages.write();
        if let Some(cp) = pages.remove(&offset) {
            self.cached_count.fetch_sub(1, Ordering::Relaxed);
            if cp.is_dirty() {
                self.dirty_count.fetch_sub(1, Ordering::Relaxed);
            }
            Some(cp.phys)
        } else {
            None
        }
    }

    /// Get offsets of dirty pages sorted by dirty_tick (oldest first) for writeback.
    /// Returns up to `limit` offsets.
    pub fn dirty_pages_by_age(&self, limit: usize) -> alloc::vec::Vec<u64> {
        let pages = self.pages.read();
        let mut dirty: alloc::vec::Vec<(u64, u64)> = pages
            .iter()
            .filter(|(_, cp)| cp.is_dirty())
            .map(|(off, cp)| (*off, cp.dirty_tick.load(Ordering::Relaxed)))
            .collect();
        dirty.sort_by_key(|&(_, tick)| tick);
        dirty.into_iter().take(limit).map(|(off, _)| off).collect()
    }

    /// Total cached pages
    pub fn cached_pages(&self) -> u64 {
        self.cached_count.load(Ordering::Relaxed)
    }

    /// Total dirty pages
    pub fn dirty_pages(&self) -> u64 {
        self.dirty_count.load(Ordering::Relaxed)
    }
}

/// — SableWire: The global page cache. Maps inode IDs to per-inode address spaces.
/// All file I/O goes through here — read checks cache first, write marks dirty.
pub struct PageCache {
    /// Map of inode_id → address space
    spaces: RwLock<BTreeMap<InodeId, Arc<AddressSpace>>>,
    /// Global dirty page count across all inodes
    pub total_dirty: AtomicU64,
    /// Global cached page count across all inodes
    pub total_cached: AtomicU64,
}

impl PageCache {
    const fn new() -> Self {
        Self {
            spaces: RwLock::new(BTreeMap::new()),
            total_dirty: AtomicU64::new(0),
            total_cached: AtomicU64::new(0),
        }
    }

    /// Get or create an address space for an inode
    pub fn get_or_create(&self, inode_id: InodeId) -> Arc<AddressSpace> {
        // — SableWire: Fast path — read lock, check if exists
        {
            let spaces = self.spaces.read();
            if let Some(space) = spaces.get(&inode_id) {
                return space.clone();
            }
        }
        // — SableWire: Slow path — write lock, insert if still missing
        let mut spaces = self.spaces.write();
        spaces
            .entry(inode_id)
            .or_insert_with(|| Arc::new(AddressSpace::new(inode_id)))
            .clone()
    }

    /// Look up a page across all address spaces. Returns the physical frame.
    pub fn find_page(&self, inode_id: InodeId, offset: u64) -> Option<PhysAddr> {
        let spaces = self.spaces.read();
        spaces.get(&inode_id)?.find_page(offset)
    }

    /// Insert a page into the cache for a given inode
    pub fn insert_page(&self, inode_id: InodeId, offset: u64, phys: PhysAddr) {
        let space = self.get_or_create(inode_id);
        let old = space.insert_page(offset, phys);
        if old.is_none() {
            self.total_cached.fetch_add(1, Ordering::Relaxed);
        }
        // — SableWire: Mark frame as page-cached in PageDB
        if let Some(db) = mm_pagedb::try_pagedb() {
            if let Some(pf) = db.get(phys) {
                pf.set_flag(mm_pagedb::PF_DIRTY);
            }
        }
    }

    /// Mark a page dirty
    pub fn mark_dirty(&self, inode_id: InodeId, offset: u64) -> bool {
        let spaces = self.spaces.read();
        if let Some(space) = spaces.get(&inode_id) {
            if space.mark_dirty(offset) {
                self.total_dirty.fetch_add(1, Ordering::Relaxed);
                return true;
            }
        }
        false
    }

    /// Invalidate a page (e.g., on truncate or eviction)
    pub fn invalidate(&self, inode_id: InodeId, offset: u64) -> Option<PhysAddr> {
        let spaces = self.spaces.read();
        if let Some(space) = spaces.get(&inode_id) {
            if let Some(phys) = space.invalidate(offset) {
                self.total_cached.fetch_sub(1, Ordering::Relaxed);
                return Some(phys);
            }
        }
        None
    }

    /// Get total cached pages across all inodes
    pub fn cached_pages(&self) -> u64 {
        self.total_cached.load(Ordering::Relaxed)
    }

    /// Get total dirty pages across all inodes
    pub fn dirty_pages(&self) -> u64 {
        self.total_dirty.load(Ordering::Relaxed)
    }

    /// Remove an entire address space (e.g., inode deleted)
    pub fn remove_inode(&self, inode_id: InodeId) {
        let mut spaces = self.spaces.write();
        if let Some(space) = spaces.remove(&inode_id) {
            let cached = space.cached_pages();
            let dirty = space.dirty_pages();
            self.total_cached.fetch_sub(cached, Ordering::Relaxed);
            self.total_dirty.fetch_sub(dirty, Ordering::Relaxed);
        }
    }

    /// Collect dirty pages across all inodes for writeback, oldest first.
    /// Returns (inode_id, offset) pairs up to `limit`.
    pub fn dirty_pages_for_writeback(&self, limit: usize) -> alloc::vec::Vec<(InodeId, u64)> {
        let spaces = self.spaces.read();
        let mut result = alloc::vec::Vec::with_capacity(limit);
        for (inode_id, space) in spaces.iter() {
            if result.len() >= limit { break; }
            let remaining = limit - result.len();
            for offset in space.dirty_pages_by_age(remaining) {
                result.push((*inode_id, offset));
            }
        }
        result
    }
}

/// — SableWire: Global page cache singleton. Zero-initialized, no runtime init needed.
static PAGE_CACHE: PageCache = PageCache::new();

/// Get a reference to the global page cache
pub fn page_cache() -> &'static PageCache {
    &PAGE_CACHE
}

/// — TorqueJax: Monotonic tick source for access/dirty timestamps.
/// Uses a simple global counter — good enough for relative ordering.
static TICK_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Get current tick (monotonically increasing)
#[inline]
fn get_tick() -> u64 {
    TICK_COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Writeback configuration thresholds
pub struct WritebackConfig {
    /// Start background writeback when dirty ratio exceeds this (percent of total cached)
    pub dirty_background_ratio: u64,
    /// Throttle writers when dirty ratio exceeds this (percent of total cached)
    pub dirty_ratio: u64,
    /// Max pages to write back per cycle
    pub max_pages_per_cycle: usize,
}

impl Default for WritebackConfig {
    fn default() -> Self {
        Self {
            dirty_background_ratio: 10,
            dirty_ratio: 40,
            max_pages_per_cycle: 32,
        }
    }
}

/// — SableWire: Check if background writeback should fire based on dirty ratio.
/// Called from scheduler idle path — NOT from ISR.
pub fn should_writeback() -> bool {
    let cached = PAGE_CACHE.cached_pages();
    if cached == 0 { return false; }
    let dirty = PAGE_CACHE.dirty_pages();
    // — SableWire: Default 10% dirty background threshold
    dirty * 100 > cached * 10
}
