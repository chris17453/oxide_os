//! Block device abstraction layer for OXIDE OS
//!
//! Provides:
//! - Block device trait
//! - I/O request queue
//! - I/O scheduler
//! - Block device registration

#![no_std]

extern crate alloc;

pub mod device;
pub mod request;
pub mod scheduler;
pub mod partition;

pub use device::{BlockDevice, BlockDeviceInfo};
pub use request::{Request, RequestType, RequestQueue};
pub use scheduler::{Scheduler, NoopScheduler};
pub use partition::Partition;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use spin::RwLock;

/// Block device error
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockError {
    /// I/O error
    IoError,
    /// Invalid block number
    InvalidBlock,
    /// Device not ready
    NotReady,
    /// Media changed
    MediaChanged,
    /// No media present
    NoMedia,
    /// Write protected
    WriteProtected,
    /// Timeout
    Timeout,
    /// Device busy
    Busy,
    /// Invalid operation
    InvalidOp,
    /// Buffer too small
    BufferTooSmall,
}

impl core::fmt::Display for BlockError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BlockError::IoError => write!(f, "I/O error"),
            BlockError::InvalidBlock => write!(f, "Invalid block number"),
            BlockError::NotReady => write!(f, "Device not ready"),
            BlockError::MediaChanged => write!(f, "Media changed"),
            BlockError::NoMedia => write!(f, "No media present"),
            BlockError::WriteProtected => write!(f, "Write protected"),
            BlockError::Timeout => write!(f, "Timeout"),
            BlockError::Busy => write!(f, "Device busy"),
            BlockError::InvalidOp => write!(f, "Invalid operation"),
            BlockError::BufferTooSmall => write!(f, "Buffer too small"),
        }
    }
}

/// Result type for block operations
pub type BlockResult<T> = Result<T, BlockError>;

/// Registered block device entry
struct RegisteredDevice {
    /// Device name (e.g., "sda", "nvme0n1")
    name: String,
    /// The block device
    device: Box<dyn BlockDevice>,
}

/// Global block device registry
static DEVICES: RwLock<Vec<RegisteredDevice>> = RwLock::new(Vec::new());

/// Register a block device
pub fn register_device(name: String, device: Box<dyn BlockDevice>) {
    let mut devices = DEVICES.write();
    devices.push(RegisteredDevice { name, device });
}

/// Unregister a block device by name
pub fn unregister_device(name: &str) -> Option<Box<dyn BlockDevice>> {
    let mut devices = DEVICES.write();
    if let Some(idx) = devices.iter().position(|d| d.name == name) {
        Some(devices.remove(idx).device)
    } else {
        None
    }
}

/// Get a reference to a block device by name
pub fn get_device(name: &str) -> Option<&'static dyn BlockDevice> {
    let devices = DEVICES.read();
    devices.iter().find(|d| d.name == name).map(|d| {
        // SAFETY: The device is stored in a static and won't be moved
        unsafe { &*(&*d.device as *const dyn BlockDevice) }
    })
}

/// List all registered block devices
pub fn list_devices() -> Vec<String> {
    DEVICES.read().iter().map(|d| d.name.clone()).collect()
}
