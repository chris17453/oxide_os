//! Device trust database

use alloc::collections::BTreeMap;
use alloc::string::String;
use spin::RwLock;
use crate::{DeviceId, MediaError, Timestamp, UsbId, ShareUrl};

/// Trust level for devices
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustLevel {
    /// Always trusted, mount read-write
    Trusted,
    /// Prompt on connect
    AskOnConnect,
    /// Always read-only
    ReadOnly,
    /// Block entirely
    Blocked,
}

impl Default for TrustLevel {
    fn default() -> Self {
        TrustLevel::AskOnConnect
    }
}

/// Device trust entry
#[derive(Debug, Clone)]
pub struct DeviceTrust {
    /// Device identifier
    pub id: DeviceId,
    /// Human-readable name
    pub name: String,
    /// Trust level
    pub trust: TrustLevel,
    /// When first seen
    pub first_seen: Timestamp,
    /// When last seen
    pub last_seen: Timestamp,
    /// Number of connections
    pub connect_count: u32,
    /// User who added trust
    pub added_by: Option<u32>,
    /// Notes
    pub notes: Option<String>,
}

impl DeviceTrust {
    /// Create new trust entry
    pub fn new(id: DeviceId, name: String, trust: TrustLevel, timestamp: Timestamp) -> Self {
        DeviceTrust {
            id,
            name,
            trust,
            first_seen: timestamp,
            last_seen: timestamp,
            connect_count: 1,
            added_by: None,
            notes: None,
        }
    }

    /// Update last seen time
    pub fn touch(&mut self, timestamp: Timestamp) {
        self.last_seen = timestamp;
        self.connect_count += 1;
    }

    /// Check if device is allowed to mount
    pub fn allows_mount(&self) -> bool {
        self.trust != TrustLevel::Blocked
    }

    /// Check if device can mount read-write
    pub fn allows_write(&self) -> bool {
        self.trust == TrustLevel::Trusted
    }
}

/// Device trust database
pub struct DeviceTrustDb {
    /// USB device trust entries
    usb_devices: RwLock<BTreeMap<String, DeviceTrust>>,
    /// Network share trust entries
    network_shares: RwLock<BTreeMap<String, DeviceTrust>>,
    /// Default trust level for unknown devices
    default_usb_trust: RwLock<TrustLevel>,
    /// Default trust level for unknown shares
    default_share_trust: RwLock<TrustLevel>,
}

impl DeviceTrustDb {
    /// Create new trust database
    pub fn new() -> Self {
        DeviceTrustDb {
            usb_devices: RwLock::new(BTreeMap::new()),
            network_shares: RwLock::new(BTreeMap::new()),
            default_usb_trust: RwLock::new(TrustLevel::ReadOnly),
            default_share_trust: RwLock::new(TrustLevel::AskOnConnect),
        }
    }

    /// Generate key for USB device
    fn usb_key(id: &UsbId) -> String {
        if let Some(ref serial) = id.serial {
            alloc::format!("{:04x}:{:04x}:{}", id.vendor_id, id.product_id, serial)
        } else {
            alloc::format!("{:04x}:{:04x}", id.vendor_id, id.product_id)
        }
    }

    /// Generate key for network share
    fn share_key(url: &ShareUrl) -> String {
        url.to_url()
    }

    /// Add or update USB device trust
    pub fn set_usb_trust(
        &self,
        id: &UsbId,
        name: String,
        trust: TrustLevel,
        timestamp: Timestamp,
    ) -> Result<(), MediaError> {
        let key = Self::usb_key(id);
        let mut devices = self.usb_devices.write();

        if let Some(entry) = devices.get_mut(&key) {
            entry.trust = trust;
            entry.touch(timestamp);
        } else {
            let entry = DeviceTrust::new(DeviceId::Usb(id.clone()), name, trust, timestamp);
            devices.insert(key, entry);
        }

        Ok(())
    }

    /// Add or update network share trust
    pub fn set_share_trust(
        &self,
        url: &ShareUrl,
        name: String,
        trust: TrustLevel,
        timestamp: Timestamp,
    ) -> Result<(), MediaError> {
        let key = Self::share_key(url);
        let mut shares = self.network_shares.write();

        if let Some(entry) = shares.get_mut(&key) {
            entry.trust = trust;
            entry.touch(timestamp);
        } else {
            let entry = DeviceTrust::new(DeviceId::Network(url.clone()), name, trust, timestamp);
            shares.insert(key, entry);
        }

        Ok(())
    }

    /// Get USB device trust
    pub fn get_usb_trust(&self, id: &UsbId) -> Option<DeviceTrust> {
        let key = Self::usb_key(id);
        self.usb_devices.read().get(&key).cloned()
    }

    /// Get network share trust
    pub fn get_share_trust(&self, url: &ShareUrl) -> Option<DeviceTrust> {
        let key = Self::share_key(url);
        self.network_shares.read().get(&key).cloned()
    }

    /// Get trust level for USB device
    pub fn usb_trust_level(&self, id: &UsbId) -> TrustLevel {
        self.get_usb_trust(id)
            .map(|t| t.trust)
            .unwrap_or_else(|| *self.default_usb_trust.read())
    }

    /// Get trust level for network share
    pub fn share_trust_level(&self, url: &ShareUrl) -> TrustLevel {
        self.get_share_trust(url)
            .map(|t| t.trust)
            .unwrap_or_else(|| *self.default_share_trust.read())
    }

    /// Remove USB device trust
    pub fn remove_usb_trust(&self, id: &UsbId) -> Result<(), MediaError> {
        let key = Self::usb_key(id);
        self.usb_devices
            .write()
            .remove(&key)
            .ok_or(MediaError::NotFound)?;
        Ok(())
    }

    /// Remove network share trust
    pub fn remove_share_trust(&self, url: &ShareUrl) -> Result<(), MediaError> {
        let key = Self::share_key(url);
        self.network_shares
            .write()
            .remove(&key)
            .ok_or(MediaError::NotFound)?;
        Ok(())
    }

    /// Block USB device
    pub fn block_usb(&self, id: &UsbId, name: String, timestamp: Timestamp) -> Result<(), MediaError> {
        self.set_usb_trust(id, name, TrustLevel::Blocked, timestamp)
    }

    /// Block network share
    pub fn block_share(&self, url: &ShareUrl, name: String, timestamp: Timestamp) -> Result<(), MediaError> {
        self.set_share_trust(url, name, TrustLevel::Blocked, timestamp)
    }

    /// Trust USB device
    pub fn trust_usb(&self, id: &UsbId, name: String, timestamp: Timestamp) -> Result<(), MediaError> {
        self.set_usb_trust(id, name, TrustLevel::Trusted, timestamp)
    }

    /// Trust network share
    pub fn trust_share(&self, url: &ShareUrl, name: String, timestamp: Timestamp) -> Result<(), MediaError> {
        self.set_share_trust(url, name, TrustLevel::Trusted, timestamp)
    }

    /// Set default USB trust level
    pub fn set_default_usb_trust(&self, trust: TrustLevel) {
        *self.default_usb_trust.write() = trust;
    }

    /// Set default share trust level
    pub fn set_default_share_trust(&self, trust: TrustLevel) {
        *self.default_share_trust.write() = trust;
    }

    /// Get all trusted USB devices
    pub fn list_usb_devices(&self) -> alloc::vec::Vec<DeviceTrust> {
        self.usb_devices.read().values().cloned().collect()
    }

    /// Get all trusted network shares
    pub fn list_network_shares(&self) -> alloc::vec::Vec<DeviceTrust> {
        self.network_shares.read().values().cloned().collect()
    }

    /// Get count of trusted USB devices
    pub fn usb_count(&self) -> usize {
        self.usb_devices.read().len()
    }

    /// Get count of trusted network shares
    pub fn share_count(&self) -> usize {
        self.network_shares.read().len()
    }
}

impl Default for DeviceTrustDb {
    fn default() -> Self {
        Self::new()
    }
}
