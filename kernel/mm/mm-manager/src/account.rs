//! Memory accounting for resource limits
//!
//! This module provides traits and utilities for tracking memory usage
//! against resource limits (e.g., cgroup memory controllers).

use mm_core::MmResult;

/// Accounting context for tracking memory usage against limits
///
/// Implementations track memory usage and enforce resource limits.
/// The memory manager calls these methods when allocating/freeing memory.
pub trait AccountingContext: Send + Sync {
    /// Check if the requested amount can be charged
    ///
    /// Returns true if charging `bytes` would not exceed the limit.
    fn can_charge(&self, bytes: u64) -> bool;

    /// Charge memory to this account
    ///
    /// Atomically adds `bytes` to the usage counter.
    /// Returns error if the charge would exceed limits.
    fn charge(&self, bytes: u64) -> MmResult<()>;

    /// Uncharge memory from this account
    ///
    /// Atomically subtracts `bytes` from the usage counter.
    fn uncharge(&self, bytes: u64);

    /// Get current memory usage in bytes
    fn usage(&self) -> u64;

    /// Get memory limit in bytes
    fn limit(&self) -> u64;

    /// Get available memory (limit - usage)
    fn available(&self) -> u64 {
        self.limit().saturating_sub(self.usage())
    }
}

/// A simple accounting context with no limits (always allows allocation)
#[derive(Debug, Default)]
pub struct UnlimitedAccount;

impl AccountingContext for UnlimitedAccount {
    fn can_charge(&self, _bytes: u64) -> bool {
        true
    }

    fn charge(&self, _bytes: u64) -> MmResult<()> {
        Ok(())
    }

    fn uncharge(&self, _bytes: u64) {}

    fn usage(&self) -> u64 {
        0
    }

    fn limit(&self) -> u64 {
        u64::MAX
    }
}

/// Kernel memory account - tracks total kernel memory usage
pub static KERNEL_ACCOUNT: UnlimitedAccount = UnlimitedAccount;
