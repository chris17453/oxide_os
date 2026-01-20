//! External Media Management for OXIDE OS
//!
//! Provides USB device detection, trust management, and mount policies.

#![no_std]

extern crate alloc;

pub mod usb;
pub mod trust;
pub mod policy;
pub mod manager;

pub use usb::*;
pub use trust::*;
pub use policy::*;
pub use manager::*;

use alloc::string::String;

/// Device identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DeviceId {
    /// USB device by vendor/product/serial
    Usb(UsbId),
    /// Network share by URL
    Network(ShareUrl),
    /// Block device path
    Block(String),
}

/// USB device identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UsbId {
    /// Vendor ID
    pub vendor_id: u16,
    /// Product ID
    pub product_id: u16,
    /// Serial number if available
    pub serial: Option<String>,
}

impl UsbId {
    /// Create new USB ID
    pub fn new(vendor_id: u16, product_id: u16, serial: Option<String>) -> Self {
        UsbId {
            vendor_id,
            product_id,
            serial,
        }
    }

    /// Check if this ID matches another (serial optional)
    pub fn matches(&self, other: &UsbId) -> bool {
        self.vendor_id == other.vendor_id
            && self.product_id == other.product_id
            && (self.serial.is_none() || other.serial.is_none() || self.serial == other.serial)
    }
}

/// Network share URL
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ShareUrl {
    /// Protocol (smb, nfs, etc.)
    pub protocol: String,
    /// Server hostname or IP
    pub server: String,
    /// Share path
    pub path: String,
}

impl ShareUrl {
    /// Create new share URL
    pub fn new(protocol: &str, server: &str, path: &str) -> Self {
        ShareUrl {
            protocol: String::from(protocol),
            server: String::from(server),
            path: String::from(path),
        }
    }

    /// Parse from string like "smb://server/share"
    pub fn parse(url: &str) -> Option<Self> {
        let parts: alloc::vec::Vec<&str> = url.splitn(2, "://").collect();
        if parts.len() != 2 {
            return None;
        }

        let protocol = parts[0];
        let rest = parts[1];

        let path_parts: alloc::vec::Vec<&str> = rest.splitn(2, '/').collect();
        let server = path_parts[0];
        let path = if path_parts.len() > 1 {
            path_parts[1]
        } else {
            ""
        };

        Some(ShareUrl {
            protocol: String::from(protocol),
            server: String::from(server),
            path: String::from(path),
        })
    }

    /// Convert to string
    pub fn to_url(&self) -> String {
        alloc::format!("{}://{}/{}", self.protocol, self.server, self.path)
    }
}

/// Mount mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountMode {
    /// Read-only mount
    ReadOnly,
    /// Read-write mount
    ReadWrite,
}

/// Device partition information
#[derive(Debug, Clone)]
pub struct Partition {
    /// Partition number
    pub number: u8,
    /// Filesystem type
    pub fs_type: Option<String>,
    /// Size in bytes
    pub size: u64,
    /// Volume label
    pub label: Option<String>,
    /// UUID
    pub uuid: Option<String>,
}

/// Timestamp (seconds since epoch)
pub type Timestamp = u64;

/// Media error
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaError {
    /// Device not found
    NotFound,
    /// Device already exists
    AlreadyExists,
    /// Permission denied
    PermissionDenied,
    /// Device is blocked
    Blocked,
    /// Invalid operation
    InvalidOperation,
    /// Mount failed
    MountFailed,
    /// Authentication required
    AuthRequired,
    /// Device busy
    Busy,
    /// IO error
    IoError,
}

impl core::fmt::Display for MediaError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotFound => write!(f, "device not found"),
            Self::AlreadyExists => write!(f, "device already exists"),
            Self::PermissionDenied => write!(f, "permission denied"),
            Self::Blocked => write!(f, "device is blocked"),
            Self::InvalidOperation => write!(f, "invalid operation"),
            Self::MountFailed => write!(f, "mount failed"),
            Self::AuthRequired => write!(f, "authentication required"),
            Self::Busy => write!(f, "device busy"),
            Self::IoError => write!(f, "I/O error"),
        }
    }
}
