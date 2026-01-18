//! CPU Cgroup Controller

use core::sync::atomic::{AtomicU64, Ordering};

/// CPU controller
pub struct CpuController {
    /// Quota in microseconds per period (0 = unlimited)
    quota_us: AtomicU64,
    /// Period in microseconds (default 100000 = 100ms)
    period_us: AtomicU64,
    /// CPU time used in current period
    usage_us: AtomicU64,
    /// Total CPU time used
    total_usage_us: AtomicU64,
    /// Number of periods with throttling
    nr_throttled: AtomicU64,
    /// Total throttled time
    throttled_us: AtomicU64,
}

impl CpuController {
    /// Create new CPU controller
    pub fn new() -> Self {
        CpuController {
            quota_us: AtomicU64::new(0), // Unlimited
            period_us: AtomicU64::new(100000), // 100ms
            usage_us: AtomicU64::new(0),
            total_usage_us: AtomicU64::new(0),
            nr_throttled: AtomicU64::new(0),
            throttled_us: AtomicU64::new(0),
        }
    }

    /// Set CPU quota
    pub fn set_quota(&self, quota_us: u64, period_us: u64) {
        self.quota_us.store(quota_us, Ordering::SeqCst);
        if period_us > 0 {
            self.period_us.store(period_us, Ordering::SeqCst);
        }
    }

    /// Get quota
    pub fn quota(&self) -> u64 {
        self.quota_us.load(Ordering::SeqCst)
    }

    /// Get period
    pub fn period(&self) -> u64 {
        self.period_us.load(Ordering::SeqCst)
    }

    /// Get CPU percentage limit (0-100, or >100 for multi-CPU)
    pub fn cpu_percent(&self) -> u64 {
        let quota = self.quota_us.load(Ordering::SeqCst);
        let period = self.period_us.load(Ordering::SeqCst);

        if quota == 0 || period == 0 {
            return 0; // Unlimited
        }

        (quota * 100) / period
    }

    /// Check if can use more CPU time
    pub fn can_use_cpu(&self, us: u64) -> bool {
        let quota = self.quota_us.load(Ordering::SeqCst);
        if quota == 0 {
            return true; // Unlimited
        }

        let current = self.usage_us.load(Ordering::SeqCst);
        current + us <= quota
    }

    /// Charge CPU time
    pub fn charge(&self, us: u64) {
        self.usage_us.fetch_add(us, Ordering::SeqCst);
        self.total_usage_us.fetch_add(us, Ordering::SeqCst);
    }

    /// Reset period (called by scheduler)
    pub fn reset_period(&self) {
        let quota = self.quota_us.load(Ordering::SeqCst);
        let usage = self.usage_us.swap(0, Ordering::SeqCst);

        // Track throttling
        if quota > 0 && usage >= quota {
            self.nr_throttled.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// Get statistics
    pub fn stats(&self) -> CpuStats {
        CpuStats {
            usage_usec: self.total_usage_us.load(Ordering::SeqCst),
            nr_periods: 0, // Would track this
            nr_throttled: self.nr_throttled.load(Ordering::SeqCst),
            throttled_usec: self.throttled_us.load(Ordering::SeqCst),
        }
    }

    /// Record throttle time
    pub fn record_throttle(&self, us: u64) {
        self.throttled_us.fetch_add(us, Ordering::SeqCst);
    }
}

impl Default for CpuController {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for CpuController {
    fn clone(&self) -> Self {
        CpuController {
            quota_us: AtomicU64::new(self.quota_us.load(Ordering::SeqCst)),
            period_us: AtomicU64::new(self.period_us.load(Ordering::SeqCst)),
            usage_us: AtomicU64::new(self.usage_us.load(Ordering::SeqCst)),
            total_usage_us: AtomicU64::new(self.total_usage_us.load(Ordering::SeqCst)),
            nr_throttled: AtomicU64::new(self.nr_throttled.load(Ordering::SeqCst)),
            throttled_us: AtomicU64::new(self.throttled_us.load(Ordering::SeqCst)),
        }
    }
}

/// CPU statistics
#[derive(Clone, Copy, Default)]
pub struct CpuStats {
    /// Total CPU usage in microseconds
    pub usage_usec: u64,
    /// Number of periods
    pub nr_periods: u64,
    /// Number of periods with throttling
    pub nr_throttled: u64,
    /// Total throttled time in microseconds
    pub throttled_usec: u64,
}
