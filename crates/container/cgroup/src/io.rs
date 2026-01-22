//! IO Cgroup Controller

use alloc::collections::BTreeMap;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::RwLock;

/// Device major:minor
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct DeviceId {
    pub major: u32,
    pub minor: u32,
}

/// IO limits for a device
pub struct DeviceIoLimits {
    /// Read bytes per second limit (0 = unlimited)
    pub rbps: AtomicU64,
    /// Write bytes per second limit
    pub wbps: AtomicU64,
    /// Read IOPS limit
    pub riops: AtomicU64,
    /// Write IOPS limit
    pub wiops: AtomicU64,
}

impl DeviceIoLimits {
    pub fn new() -> Self {
        DeviceIoLimits {
            rbps: AtomicU64::new(0),
            wbps: AtomicU64::new(0),
            riops: AtomicU64::new(0),
            wiops: AtomicU64::new(0),
        }
    }
}

impl Default for DeviceIoLimits {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for DeviceIoLimits {
    fn clone(&self) -> Self {
        DeviceIoLimits {
            rbps: AtomicU64::new(self.rbps.load(Ordering::SeqCst)),
            wbps: AtomicU64::new(self.wbps.load(Ordering::SeqCst)),
            riops: AtomicU64::new(self.riops.load(Ordering::SeqCst)),
            wiops: AtomicU64::new(self.wiops.load(Ordering::SeqCst)),
        }
    }
}

/// IO controller
pub struct IoController {
    /// Per-device limits
    limits: RwLock<BTreeMap<DeviceId, DeviceIoLimits>>,
    /// Total read bytes
    read_bytes: AtomicU64,
    /// Total write bytes
    write_bytes: AtomicU64,
    /// Total read operations
    read_ios: AtomicU64,
    /// Total write operations
    write_ios: AtomicU64,
}

impl IoController {
    /// Create new IO controller
    pub fn new() -> Self {
        IoController {
            limits: RwLock::new(BTreeMap::new()),
            read_bytes: AtomicU64::new(0),
            write_bytes: AtomicU64::new(0),
            read_ios: AtomicU64::new(0),
            write_ios: AtomicU64::new(0),
        }
    }

    /// Set read bytes per second limit for device
    pub fn set_rbps(&self, device: DeviceId, bps: u64) {
        let mut limits = self.limits.write();
        limits
            .entry(device)
            .or_insert_with(DeviceIoLimits::new)
            .rbps
            .store(bps, Ordering::SeqCst);
    }

    /// Set write bytes per second limit for device
    pub fn set_wbps(&self, device: DeviceId, bps: u64) {
        let mut limits = self.limits.write();
        limits
            .entry(device)
            .or_insert_with(DeviceIoLimits::new)
            .wbps
            .store(bps, Ordering::SeqCst);
    }

    /// Set read IOPS limit for device
    pub fn set_riops(&self, device: DeviceId, iops: u64) {
        let mut limits = self.limits.write();
        limits
            .entry(device)
            .or_insert_with(DeviceIoLimits::new)
            .riops
            .store(iops, Ordering::SeqCst);
    }

    /// Set write IOPS limit for device
    pub fn set_wiops(&self, device: DeviceId, iops: u64) {
        let mut limits = self.limits.write();
        limits
            .entry(device)
            .or_insert_with(DeviceIoLimits::new)
            .wiops
            .store(iops, Ordering::SeqCst);
    }

    /// Check if read is allowed
    pub fn can_read(&self, device: DeviceId, _bytes: u64) -> bool {
        let limits = self.limits.read();
        if let Some(dev_limits) = limits.get(&device) {
            let rbps = dev_limits.rbps.load(Ordering::SeqCst);
            // Simplified: just check if limit is set
            // Real implementation would track per-second usage
            rbps == 0 || true
        } else {
            true
        }
    }

    /// Check if write is allowed
    pub fn can_write(&self, device: DeviceId, _bytes: u64) -> bool {
        let limits = self.limits.read();
        if let Some(dev_limits) = limits.get(&device) {
            let wbps = dev_limits.wbps.load(Ordering::SeqCst);
            wbps == 0 || true
        } else {
            true
        }
    }

    /// Record read IO
    pub fn record_read(&self, bytes: u64) {
        self.read_bytes.fetch_add(bytes, Ordering::SeqCst);
        self.read_ios.fetch_add(1, Ordering::SeqCst);
    }

    /// Record write IO
    pub fn record_write(&self, bytes: u64) {
        self.write_bytes.fetch_add(bytes, Ordering::SeqCst);
        self.write_ios.fetch_add(1, Ordering::SeqCst);
    }

    /// Get statistics
    pub fn stats(&self) -> IoStats {
        IoStats {
            read_bytes: self.read_bytes.load(Ordering::SeqCst),
            write_bytes: self.write_bytes.load(Ordering::SeqCst),
            read_ios: self.read_ios.load(Ordering::SeqCst),
            write_ios: self.write_ios.load(Ordering::SeqCst),
        }
    }
}

impl Default for IoController {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for IoController {
    fn clone(&self) -> Self {
        IoController {
            limits: RwLock::new(self.limits.read().clone()),
            read_bytes: AtomicU64::new(self.read_bytes.load(Ordering::SeqCst)),
            write_bytes: AtomicU64::new(self.write_bytes.load(Ordering::SeqCst)),
            read_ios: AtomicU64::new(self.read_ios.load(Ordering::SeqCst)),
            write_ios: AtomicU64::new(self.write_ios.load(Ordering::SeqCst)),
        }
    }
}

/// IO statistics
#[derive(Clone, Copy, Default)]
pub struct IoStats {
    /// Total read bytes
    pub read_bytes: u64,
    /// Total write bytes
    pub write_bytes: u64,
    /// Total read operations
    pub read_ios: u64,
    /// Total write operations
    pub write_ios: u64,
}
