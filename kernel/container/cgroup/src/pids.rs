//! PIDs Cgroup Controller

use core::sync::atomic::{AtomicU64, Ordering};

/// PIDs controller
pub struct PidsController {
    /// Maximum PIDs (0 = unlimited)
    max_pids: AtomicU64,
    /// Current PID count
    current_pids: AtomicU64,
    /// Events (failed forks due to limit)
    events_max: AtomicU64,
}

impl PidsController {
    /// Create new PIDs controller
    pub fn new() -> Self {
        PidsController {
            max_pids: AtomicU64::new(0), // Unlimited
            current_pids: AtomicU64::new(0),
            events_max: AtomicU64::new(0),
        }
    }

    /// Set maximum PIDs
    pub fn set_max(&self, max: u64) {
        self.max_pids.store(max, Ordering::SeqCst);
    }

    /// Get maximum PIDs
    pub fn max(&self) -> u64 {
        self.max_pids.load(Ordering::SeqCst)
    }

    /// Get current PID count
    pub fn current(&self) -> u64 {
        self.current_pids.load(Ordering::SeqCst)
    }

    /// Check if can fork
    pub fn can_fork(&self) -> bool {
        let max = self.max_pids.load(Ordering::SeqCst);
        if max == 0 {
            return true; // Unlimited
        }

        let current = self.current_pids.load(Ordering::SeqCst);
        current < max
    }

    /// Add a task
    pub fn add_task(&self) {
        self.current_pids.fetch_add(1, Ordering::SeqCst);
    }

    /// Remove a task
    pub fn remove_task(&self) {
        self.current_pids.fetch_sub(1, Ordering::SeqCst);
    }

    /// Record failed fork
    pub fn record_fork_failure(&self) {
        self.events_max.fetch_add(1, Ordering::SeqCst);
    }

    /// Get events count
    pub fn events(&self) -> u64 {
        self.events_max.load(Ordering::SeqCst)
    }

    /// Get statistics
    pub fn stats(&self) -> PidsStats {
        PidsStats {
            current: self.current_pids.load(Ordering::SeqCst),
            max: self.max_pids.load(Ordering::SeqCst),
            events_max: self.events_max.load(Ordering::SeqCst),
        }
    }
}

impl Default for PidsController {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for PidsController {
    fn clone(&self) -> Self {
        PidsController {
            max_pids: AtomicU64::new(self.max_pids.load(Ordering::SeqCst)),
            current_pids: AtomicU64::new(self.current_pids.load(Ordering::SeqCst)),
            events_max: AtomicU64::new(self.events_max.load(Ordering::SeqCst)),
        }
    }
}

/// PIDs statistics
#[derive(Clone, Copy, Default)]
pub struct PidsStats {
    /// Current number of PIDs
    pub current: u64,
    /// Maximum PIDs limit
    pub max: u64,
    /// Number of times fork was denied
    pub events_max: u64,
}
