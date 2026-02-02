//! Memory allocation statistics tracking

use core::sync::atomic::{AtomicU64, Ordering};

/// Memory statistics for tracking allocations
#[derive(Debug)]
pub struct MemoryStats {
    /// Total physical memory in bytes
    pub total_bytes: AtomicU64,
    /// Free physical memory in bytes
    pub free_bytes: AtomicU64,
    /// Total allocations performed
    pub alloc_count: AtomicU64,
    /// Total frees performed
    pub free_count: AtomicU64,
    /// Failed allocation attempts
    pub alloc_failures: AtomicU64,
}

impl MemoryStats {
    /// Create new zeroed statistics
    pub const fn new() -> Self {
        Self {
            total_bytes: AtomicU64::new(0),
            free_bytes: AtomicU64::new(0),
            alloc_count: AtomicU64::new(0),
            free_count: AtomicU64::new(0),
            alloc_failures: AtomicU64::new(0),
        }
    }

    /// Record an allocation
    pub fn record_alloc(&self, bytes: u64) {
        self.free_bytes.fetch_sub(bytes, Ordering::Relaxed);
        self.alloc_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a free
    pub fn record_free(&self, bytes: u64) {
        self.free_bytes.fetch_add(bytes, Ordering::Relaxed);
        self.free_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a failed allocation
    pub fn record_failure(&self) {
        self.alloc_failures.fetch_add(1, Ordering::Relaxed);
    }

    /// Get total memory in bytes
    pub fn total(&self) -> u64 {
        self.total_bytes.load(Ordering::Relaxed)
    }

    /// Get free memory in bytes
    pub fn free(&self) -> u64 {
        self.free_bytes.load(Ordering::Relaxed)
    }

    /// Get used memory in bytes
    pub fn used(&self) -> u64 {
        self.total() - self.free()
    }

    /// Get total allocation count
    pub fn allocations(&self) -> u64 {
        self.alloc_count.load(Ordering::Relaxed)
    }

    /// Get total free count
    pub fn frees(&self) -> u64 {
        self.free_count.load(Ordering::Relaxed)
    }

    /// Get failure count
    pub fn failures(&self) -> u64 {
        self.alloc_failures.load(Ordering::Relaxed)
    }

    /// Initialize with total memory
    pub fn init(&self, total: u64, free: u64) {
        self.total_bytes.store(total, Ordering::Relaxed);
        self.free_bytes.store(free, Ordering::Relaxed);
    }
}

impl Default for MemoryStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Per-zone statistics
#[derive(Debug)]
pub struct ZoneStats {
    /// Free pages at each order level
    pub free_pages: [AtomicU64; 11],
    /// Total pages in zone
    pub total_pages: AtomicU64,
}

impl ZoneStats {
    /// Create new zeroed zone statistics
    pub const fn new() -> Self {
        Self {
            free_pages: [
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
            ],
            total_pages: AtomicU64::new(0),
        }
    }

    /// Get total free pages in zone
    pub fn total_free_pages(&self) -> u64 {
        let mut total = 0u64;
        for (order, count) in self.free_pages.iter().enumerate() {
            total += count.load(Ordering::Relaxed) * (1u64 << order);
        }
        total
    }
}

impl Default for ZoneStats {
    fn default() -> Self {
        Self::new()
    }
}
