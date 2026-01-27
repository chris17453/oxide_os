//! Memory zones for physical memory management
//!
//! Zones partition physical memory by characteristics:
//! - DMA: Low memory (< 16MB) for legacy DMA devices
//! - Normal: Regular memory (16MB - 4GB)
//! - High: High memory (> 4GB) for 64-bit systems

use crate::stats::ZoneStats;
use crate::{FRAME_SIZE, MAX_ORDER};
use os_core::PhysAddr;

/// Memory zone types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoneType {
    /// DMA zone: memory below 16MB for legacy ISA DMA
    Dma,
    /// Normal zone: regular addressable memory (16MB - 4GB)
    Normal,
    /// High zone: memory above 4GB (64-bit only)
    High,
}

impl ZoneType {
    /// Get the zone for a physical address
    pub fn for_address(addr: PhysAddr) -> Self {
        let addr = addr.as_u64();
        if addr < DMA_ZONE_END {
            ZoneType::Dma
        } else if addr < NORMAL_ZONE_END {
            ZoneType::Normal
        } else {
            ZoneType::High
        }
    }

    /// Get zone index (for array indexing)
    pub fn index(self) -> usize {
        match self {
            ZoneType::Dma => 0,
            ZoneType::Normal => 1,
            ZoneType::High => 2,
        }
    }

    /// Get zone from index
    pub fn from_index(idx: usize) -> Option<Self> {
        match idx {
            0 => Some(ZoneType::Dma),
            1 => Some(ZoneType::Normal),
            2 => Some(ZoneType::High),
            _ => None,
        }
    }

    /// Get the start address of this zone
    pub fn start(self) -> u64 {
        match self {
            ZoneType::Dma => 0,
            ZoneType::Normal => DMA_ZONE_END,
            ZoneType::High => NORMAL_ZONE_END,
        }
    }

    /// Get the end address of this zone
    pub fn end(self) -> u64 {
        match self {
            ZoneType::Dma => DMA_ZONE_END,
            ZoneType::Normal => NORMAL_ZONE_END,
            ZoneType::High => u64::MAX,
        }
    }
}

/// DMA zone boundary (16 MB)
pub const DMA_ZONE_END: u64 = 16 * 1024 * 1024;

/// Normal zone boundary (4 GB)
pub const NORMAL_ZONE_END: u64 = 4 * 1024 * 1024 * 1024;

/// A memory zone containing a buddy allocator free list
pub struct MemoryZone {
    /// Zone type
    pub zone_type: ZoneType,
    /// Base address of this zone's managed memory
    pub base: PhysAddr,
    /// Size of managed memory in bytes
    pub size: u64,
    /// Free lists for each order (order 0 = 4KB, order 10 = 4MB)
    /// Each entry points to the head of a linked list of free blocks
    pub free_lists: [FreeListHead; MAX_ORDER + 1],
    /// Statistics for this zone
    pub stats: ZoneStats,
}

/// Head of a free list
#[derive(Debug)]
pub struct FreeListHead {
    /// First free block (physical frame number), or 0 if empty
    pub head: u64,
    /// Number of free blocks at this order
    pub count: u64,
}

impl FreeListHead {
    /// Create an empty free list head
    pub const fn new() -> Self {
        Self { head: 0, count: 0 }
    }

    /// Check if the list is empty
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}

impl Default for FreeListHead {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryZone {
    /// Create a new uninitialized memory zone
    pub const fn new(zone_type: ZoneType) -> Self {
        Self {
            zone_type,
            base: PhysAddr::new(0),
            size: 0,
            free_lists: [
                FreeListHead::new(),
                FreeListHead::new(),
                FreeListHead::new(),
                FreeListHead::new(),
                FreeListHead::new(),
                FreeListHead::new(),
                FreeListHead::new(),
                FreeListHead::new(),
                FreeListHead::new(),
                FreeListHead::new(),
                FreeListHead::new(),
            ],
            stats: ZoneStats::new(),
        }
    }

    /// Initialize the zone with a memory region
    pub fn init(&mut self, base: PhysAddr, size: u64) {
        self.base = base;
        self.size = size;
        let pages = size / FRAME_SIZE as u64;
        self.stats.total_pages.store(pages, core::sync::atomic::Ordering::Relaxed);
    }

    /// Get the number of pages at a given order
    pub fn free_pages_at_order(&self, order: usize) -> u64 {
        if order > MAX_ORDER {
            return 0;
        }
        self.free_lists[order].count
    }

    /// Get total free pages (accounting for block sizes)
    pub fn total_free_pages(&self) -> u64 {
        let mut total = 0u64;
        for (order, list) in self.free_lists.iter().enumerate() {
            // Each block at order N contains 2^N pages
            total += list.count * (1u64 << order);
        }
        total
    }

    /// Get total free bytes
    pub fn free_bytes(&self) -> u64 {
        self.total_free_pages() * FRAME_SIZE as u64
    }

    /// Check if this zone contains an address
    pub fn contains(&self, addr: PhysAddr) -> bool {
        let a = addr.as_u64();
        a >= self.base.as_u64() && a < self.base.as_u64() + self.size
    }

    /// Check if the zone is empty (no memory assigned)
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }
}

/// Allocation request specifying requirements
#[derive(Debug, Clone, Copy)]
pub struct AllocRequest {
    /// Minimum order (0 = 4KB page)
    pub order: usize,
    /// Preferred zone (will fall back to other zones)
    pub zone: Option<ZoneType>,
    /// Flags for the allocation
    pub flags: AllocFlags,
}

impl AllocRequest {
    /// Create a simple allocation request for a given order
    pub const fn new(order: usize) -> Self {
        Self {
            order,
            zone: None,
            flags: AllocFlags::empty(),
        }
    }

    /// Create a request for contiguous pages
    pub const fn pages(count: usize) -> Self {
        // Find the smallest order that can satisfy this request
        let order = pages_to_order(count);
        Self::new(order)
    }

    /// Request allocation from DMA zone
    pub const fn dma(order: usize) -> Self {
        Self {
            order,
            zone: Some(ZoneType::Dma),
            flags: AllocFlags::empty(),
        }
    }

    /// Request allocation that must succeed (can reclaim)
    pub const fn critical(order: usize) -> Self {
        Self {
            order,
            zone: None,
            flags: AllocFlags::MUST_SUCCEED,
        }
    }
}

/// Convert page count to minimum order
pub const fn pages_to_order(pages: usize) -> usize {
    if pages == 0 {
        return 0;
    }
    // Find smallest power of 2 >= pages
    let mut order = 0;
    let mut size = 1;
    while size < pages && order < MAX_ORDER {
        size <<= 1;
        order += 1;
    }
    order
}

bitflags::bitflags! {
    /// Flags for allocation requests
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct AllocFlags: u32 {
        /// Allocation must succeed (can trigger reclaim)
        const MUST_SUCCEED = 1 << 0;
        /// Zero the allocated memory
        const ZERO = 1 << 1;
        /// Allocation is for user space
        const USER = 1 << 2;
        /// Allocation is for DMA
        const DMA = 1 << 3;
    }
}

