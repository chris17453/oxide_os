//! Memory Cgroup Controller

use core::sync::atomic::{AtomicU64, Ordering};

/// Memory controller
pub struct MemoryController {
    /// Memory limit (0 = unlimited)
    max_bytes: AtomicU64,
    /// Low memory threshold (for memory pressure)
    low_bytes: AtomicU64,
    /// High memory threshold (triggers reclaim)
    high_bytes: AtomicU64,
    /// Current memory usage
    current_bytes: AtomicU64,
    /// Swap limit (0 = same as memory limit)
    swap_max_bytes: AtomicU64,
    /// Current swap usage
    swap_current_bytes: AtomicU64,
    /// OOM kill count
    oom_kill_count: AtomicU64,
    /// OOM disabled
    oom_disabled: AtomicU64,
}

impl MemoryController {
    /// Create new memory controller
    pub fn new() -> Self {
        MemoryController {
            max_bytes: AtomicU64::new(0), // Unlimited
            low_bytes: AtomicU64::new(0),
            high_bytes: AtomicU64::new(0),
            current_bytes: AtomicU64::new(0),
            swap_max_bytes: AtomicU64::new(0),
            swap_current_bytes: AtomicU64::new(0),
            oom_kill_count: AtomicU64::new(0),
            oom_disabled: AtomicU64::new(0),
        }
    }

    /// Set memory limit
    pub fn set_max(&self, max_bytes: u64) {
        self.max_bytes.store(max_bytes, Ordering::SeqCst);
    }

    /// Get memory limit
    pub fn max(&self) -> u64 {
        self.max_bytes.load(Ordering::SeqCst)
    }

    /// Set low threshold
    pub fn set_low(&self, low_bytes: u64) {
        self.low_bytes.store(low_bytes, Ordering::SeqCst);
    }

    /// Set high threshold
    pub fn set_high(&self, high_bytes: u64) {
        self.high_bytes.store(high_bytes, Ordering::SeqCst);
    }

    /// Get current usage
    pub fn current(&self) -> u64 {
        self.current_bytes.load(Ordering::SeqCst)
    }

    /// Check if can charge memory
    pub fn can_charge(&self, bytes: u64) -> bool {
        let max = self.max_bytes.load(Ordering::SeqCst);
        if max == 0 {
            return true; // Unlimited
        }

        let current = self.current_bytes.load(Ordering::SeqCst);
        current + bytes <= max
    }

    /// Charge memory usage
    pub fn charge(&self, bytes: u64) {
        self.current_bytes.fetch_add(bytes, Ordering::SeqCst);
    }

    /// Uncharge memory usage
    pub fn uncharge(&self, bytes: u64) {
        self.current_bytes.fetch_sub(bytes, Ordering::SeqCst);
    }

    /// Set swap limit
    pub fn set_swap_max(&self, max_bytes: u64) {
        self.swap_max_bytes.store(max_bytes, Ordering::SeqCst);
    }

    /// Get swap usage
    pub fn swap_current(&self) -> u64 {
        self.swap_current_bytes.load(Ordering::SeqCst)
    }

    /// Charge swap usage
    pub fn charge_swap(&self, bytes: u64) -> bool {
        let max = self.swap_max_bytes.load(Ordering::SeqCst);
        let current = self.swap_current_bytes.load(Ordering::SeqCst);

        if max > 0 && current + bytes > max {
            return false;
        }

        self.swap_current_bytes.fetch_add(bytes, Ordering::SeqCst);
        true
    }

    /// Uncharge swap
    pub fn uncharge_swap(&self, bytes: u64) {
        self.swap_current_bytes.fetch_sub(bytes, Ordering::SeqCst);
    }

    /// Check if under memory pressure
    pub fn under_pressure(&self) -> bool {
        let current = self.current_bytes.load(Ordering::SeqCst);
        let high = self.high_bytes.load(Ordering::SeqCst);
        high > 0 && current > high
    }

    /// Record OOM kill
    pub fn record_oom_kill(&self) {
        self.oom_kill_count.fetch_add(1, Ordering::SeqCst);
    }

    /// Get OOM kill count
    pub fn oom_kill_count(&self) -> u64 {
        self.oom_kill_count.load(Ordering::SeqCst)
    }

    /// Disable OOM killer
    pub fn set_oom_disabled(&self, disabled: bool) {
        self.oom_disabled.store(if disabled { 1 } else { 0 }, Ordering::SeqCst);
    }

    /// Check if OOM killer is disabled
    pub fn is_oom_disabled(&self) -> bool {
        self.oom_disabled.load(Ordering::SeqCst) != 0
    }

    /// Get statistics
    pub fn stats(&self) -> MemoryStats {
        MemoryStats {
            current: self.current_bytes.load(Ordering::SeqCst),
            min: 0,
            low: self.low_bytes.load(Ordering::SeqCst),
            high: self.high_bytes.load(Ordering::SeqCst),
            max: self.max_bytes.load(Ordering::SeqCst),
            swap_current: self.swap_current_bytes.load(Ordering::SeqCst),
            swap_max: self.swap_max_bytes.load(Ordering::SeqCst),
            oom_kill: self.oom_kill_count.load(Ordering::SeqCst),
        }
    }
}

impl Default for MemoryController {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for MemoryController {
    fn clone(&self) -> Self {
        MemoryController {
            max_bytes: AtomicU64::new(self.max_bytes.load(Ordering::SeqCst)),
            low_bytes: AtomicU64::new(self.low_bytes.load(Ordering::SeqCst)),
            high_bytes: AtomicU64::new(self.high_bytes.load(Ordering::SeqCst)),
            current_bytes: AtomicU64::new(self.current_bytes.load(Ordering::SeqCst)),
            swap_max_bytes: AtomicU64::new(self.swap_max_bytes.load(Ordering::SeqCst)),
            swap_current_bytes: AtomicU64::new(self.swap_current_bytes.load(Ordering::SeqCst)),
            oom_kill_count: AtomicU64::new(self.oom_kill_count.load(Ordering::SeqCst)),
            oom_disabled: AtomicU64::new(self.oom_disabled.load(Ordering::SeqCst)),
        }
    }
}

/// Memory statistics
#[derive(Clone, Copy, Default)]
pub struct MemoryStats {
    /// Current memory usage
    pub current: u64,
    /// Minimum guaranteed memory
    pub min: u64,
    /// Low threshold
    pub low: u64,
    /// High threshold
    pub high: u64,
    /// Maximum limit
    pub max: u64,
    /// Current swap usage
    pub swap_current: u64,
    /// Swap limit
    pub swap_max: u64,
    /// OOM kill count
    pub oom_kill: u64,
}
