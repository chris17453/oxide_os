//! Pending signal queue
//!
//! Tracks signals that have been sent but not yet delivered.

use crate::action::SigInfo;
use crate::sigset::SigSet;
use crate::signal::NSIG;

/// A pending signal with optional siginfo
#[derive(Debug, Clone, Copy)]
pub struct PendingSignal {
    /// Signal number
    pub signo: i32,
    /// Signal info (if available)
    pub info: Option<SigInfo>,
}

impl PendingSignal {
    pub fn new(signo: i32) -> Self {
        PendingSignal { signo, info: None }
    }

    pub fn with_info(signo: i32, info: SigInfo) -> Self {
        PendingSignal {
            signo,
            info: Some(info),
        }
    }
}

/// Pending signals for a process/thread
#[derive(Debug, Clone)]
pub struct PendingSignals {
    /// Bitmask of pending signals (for quick check)
    pending: SigSet,
    /// Signal info for each pending signal (for SA_SIGINFO)
    /// Only stores the most recent info for each signal
    info: [Option<SigInfo>; NSIG],
}

impl Default for PendingSignals {
    fn default() -> Self {
        Self::new()
    }
}

impl PendingSignals {
    /// Create empty pending signals
    pub fn new() -> Self {
        PendingSignals {
            pending: SigSet::empty(),
            info: [None; NSIG],
        }
    }

    /// Add a pending signal
    pub fn add(&mut self, sig: i32, info: Option<SigInfo>) {
        if sig >= 1 && sig <= NSIG as i32 {
            self.pending.add(sig);
            self.info[(sig - 1) as usize] = info;
        }
    }

    /// Remove a pending signal
    pub fn remove(&mut self, sig: i32) -> Option<SigInfo> {
        if sig >= 1 && sig <= NSIG as i32 {
            self.pending.remove(sig);
            self.info[(sig - 1) as usize].take()
        } else {
            None
        }
    }

    /// Check if a signal is pending
    pub fn is_pending(&self, sig: i32) -> bool {
        self.pending.contains(sig)
    }

    /// Get the set of pending signals
    pub fn set(&self) -> SigSet {
        self.pending
    }

    /// Get pending signals that are not blocked
    pub fn deliverable(&self, blocked: &SigSet) -> SigSet {
        self.pending.difference(blocked)
    }

    /// Get the first deliverable signal
    pub fn next_deliverable(&self, blocked: &SigSet) -> Option<i32> {
        self.deliverable(blocked).first()
    }

    /// Dequeue the first deliverable signal
    pub fn dequeue(&mut self, blocked: &SigSet) -> Option<PendingSignal> {
        if let Some(sig) = self.next_deliverable(blocked) {
            let info = self.remove(sig);
            Some(PendingSignal { signo: sig, info })
        } else {
            None
        }
    }

    /// Clear all pending signals
    pub fn clear(&mut self) {
        self.pending.clear();
        for i in 0..NSIG {
            self.info[i] = None;
        }
    }

    /// Check if there are any pending signals
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Check if there are any deliverable signals
    pub fn has_deliverable(&self, blocked: &SigSet) -> bool {
        !self.deliverable(blocked).is_empty()
    }

    /// Get siginfo for a signal (if available)
    pub fn get_info(&self, sig: i32) -> Option<&SigInfo> {
        if sig >= 1 && sig <= NSIG as i32 {
            self.info[(sig - 1) as usize].as_ref()
        } else {
            None
        }
    }
}
