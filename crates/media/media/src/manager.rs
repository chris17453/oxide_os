//! Media manager for device tracking and mounting

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::RwLock;

use crate::{
    DeviceId, DeviceTrustDb, MediaError, MediaPolicy, MountMode, TrustLevel,
    UsbDevice, UsbEvent, UsbEventType, UsbMonitor,
};

/// Active mount information
#[derive(Debug, Clone)]
pub struct ActiveMount {
    /// Device identifier
    pub device_id: DeviceId,
    /// Mount point path
    pub mount_point: String,
    /// Current mount mode
    pub mode: MountMode,
    /// When mounted
    pub mounted_at: u64,
    /// Last activity timestamp
    pub last_activity: u64,
    /// Original trust level
    pub trust_level: TrustLevel,
}

impl ActiveMount {
    /// Create new active mount
    pub fn new(
        device_id: DeviceId,
        mount_point: String,
        mode: MountMode,
        timestamp: u64,
        trust_level: TrustLevel,
    ) -> Self {
        ActiveMount {
            device_id,
            mount_point,
            mode,
            mounted_at: timestamp,
            last_activity: timestamp,
            trust_level,
        }
    }

    /// Update activity timestamp
    pub fn touch(&mut self, timestamp: u64) {
        self.last_activity = timestamp;
    }

    /// Check if mount is read-only
    pub fn is_read_only(&self) -> bool {
        self.mode == MountMode::ReadOnly
    }

    /// Check if mount is stale (inactive for given minutes)
    pub fn is_stale(&self, current_time: u64, timeout_minutes: u32) -> bool {
        let timeout_seconds = timeout_minutes as u64 * 60;
        current_time > self.last_activity + timeout_seconds
    }
}

/// Media event handler trait
pub trait MediaEventHandler: Send + Sync {
    /// Called when a device is mounted
    fn on_mounted(&self, mount: &ActiveMount);

    /// Called when a device is unmounted
    fn on_unmounted(&self, mount_point: &str);

    /// Called when a promotion is requested
    fn on_promotion_requested(&self, mount_point: &str);

    /// Called when a promotion is granted
    fn on_promotion_granted(&self, mount_point: &str);
}

/// Media manager
pub struct MediaManager {
    /// Known USB devices
    usb_devices: RwLock<BTreeMap<String, UsbDevice>>,
    /// Active mounts
    mounts: RwLock<BTreeMap<String, ActiveMount>>,
    /// Trust database
    trust_db: Arc<DeviceTrustDb>,
    /// Mount policy
    policy: RwLock<MediaPolicy>,
    /// Event handlers
    handlers: RwLock<Vec<Arc<dyn MediaEventHandler>>>,
    /// Next mount ID
    next_mount_id: RwLock<u64>,
    /// Mount base path
    mount_base: RwLock<String>,
}

impl MediaManager {
    /// Create new media manager
    pub fn new(trust_db: Arc<DeviceTrustDb>) -> Self {
        MediaManager {
            usb_devices: RwLock::new(BTreeMap::new()),
            mounts: RwLock::new(BTreeMap::new()),
            trust_db,
            policy: RwLock::new(MediaPolicy::new()),
            handlers: RwLock::new(Vec::new()),
            next_mount_id: RwLock::new(0),
            mount_base: RwLock::new(String::from("/media")),
        }
    }

    /// Set mount base path
    pub fn set_mount_base(&self, base: &str) {
        *self.mount_base.write() = String::from(base);
    }

    /// Set policy
    pub fn set_policy(&self, policy: MediaPolicy) {
        *self.policy.write() = policy;
    }

    /// Get policy
    pub fn policy(&self) -> MediaPolicy {
        self.policy.read().clone()
    }

    /// Add event handler
    pub fn add_handler(&self, handler: Arc<dyn MediaEventHandler>) {
        self.handlers.write().push(handler);
    }

    /// Handle USB device event
    pub fn handle_usb_event(&self, event: UsbEvent) -> Result<(), MediaError> {
        match event.event_type {
            UsbEventType::Connected => self.on_device_connected(event.device, event.timestamp),
            UsbEventType::Disconnected => self.on_device_disconnected(&event.device.path),
            UsbEventType::ConfigChanged => Ok(()), // Handle if needed
        }
    }

    /// Called when a USB device is connected
    fn on_device_connected(&self, device: UsbDevice, timestamp: u64) -> Result<(), MediaError> {
        let path = device.path.clone();
        let usb_id = device.id();

        // Store device
        self.usb_devices
            .write()
            .insert(path.clone(), device.clone());

        // Check if it's a mass storage device
        if !device.is_mass_storage() {
            return Ok(());
        }

        // Get trust level
        let trust = self.trust_db.usb_trust_level(&usb_id);

        // Check if blocked
        if trust == TrustLevel::Blocked {
            return Err(MediaError::Blocked);
        }

        // Determine mount mode
        let policy = self.policy.read();
        let mode = policy.mount_mode_for_trust(trust);

        // Generate mount point
        let mount_point = self.generate_mount_point(&device);

        // Create mount entry
        let mount = ActiveMount::new(
            DeviceId::Usb(usb_id.clone()),
            mount_point.clone(),
            mode,
            timestamp,
            trust,
        );

        // Store mount
        self.mounts
            .write()
            .insert(mount_point.clone(), mount.clone());

        // Update trust DB (record seeing this device)
        let _ = self
            .trust_db
            .set_usb_trust(&usb_id, device.display_name(), trust, timestamp);

        // Notify handlers
        let handlers = self.handlers.read();
        for handler in handlers.iter() {
            handler.on_mounted(&mount);
        }

        Ok(())
    }

    /// Called when a USB device is disconnected
    fn on_device_disconnected(&self, device_path: &str) -> Result<(), MediaError> {
        // Remove device
        self.usb_devices.write().remove(device_path);

        // Find and remove mount
        let mount_point = {
            let mounts = self.mounts.read();
            mounts
                .iter()
                .find(|(_, m)| {
                    if let DeviceId::Usb(_) = &m.device_id {
                        // In real implementation, match by device path
                        true
                    } else {
                        false
                    }
                })
                .map(|(k, _)| k.clone())
        };

        if let Some(mp) = mount_point {
            self.mounts.write().remove(&mp);

            // Notify handlers
            let handlers = self.handlers.read();
            for handler in handlers.iter() {
                handler.on_unmounted(&mp);
            }
        }

        Ok(())
    }

    /// Generate mount point for device
    fn generate_mount_point(&self, device: &UsbDevice) -> String {
        let base = self.mount_base.read();
        let mut id = self.next_mount_id.write();
        let mount_id = *id;
        *id += 1;

        // Use device label if available, otherwise use ID
        if let Some(partition) = device.partitions.first() {
            if let Some(ref label) = partition.label {
                return alloc::format!("{}/{}", base, label);
            }
        }

        alloc::format!("{}/usb{}", base, mount_id)
    }

    /// Request promotion to read-write
    pub fn request_promotion(&self, mount_point: &str) -> Result<bool, MediaError> {
        let policy = self.policy.read();

        let mount = self
            .mounts
            .read()
            .get(mount_point)
            .cloned()
            .ok_or(MediaError::NotFound)?;

        // Check if already RW
        if mount.mode == MountMode::ReadWrite {
            return Ok(true);
        }

        // Check if promotion is allowed
        if !policy.allows_promotion(mount.trust_level) {
            return Err(MediaError::PermissionDenied);
        }

        // Notify handlers
        let handlers = self.handlers.read();
        for handler in handlers.iter() {
            handler.on_promotion_requested(mount_point);
        }

        // Return whether auth is required
        Ok(!policy.require_auth_for_rw)
    }

    /// Grant promotion (after authentication)
    pub fn grant_promotion(&self, mount_point: &str, timestamp: u64) -> Result<(), MediaError> {
        let mut mounts = self.mounts.write();
        let mount = mounts.get_mut(mount_point).ok_or(MediaError::NotFound)?;

        mount.mode = MountMode::ReadWrite;
        mount.last_activity = timestamp;

        // Notify handlers
        let handlers = self.handlers.read();
        for handler in handlers.iter() {
            handler.on_promotion_granted(mount_point);
        }

        Ok(())
    }

    /// Unmount device
    pub fn unmount(&self, mount_point: &str) -> Result<(), MediaError> {
        let mount = self
            .mounts
            .write()
            .remove(mount_point)
            .ok_or(MediaError::NotFound)?;

        // Notify handlers
        let handlers = self.handlers.read();
        for handler in handlers.iter() {
            handler.on_unmounted(mount_point);
        }

        // Mark device as safe to remove
        drop(mount);

        Ok(())
    }

    /// Trust device at mount point
    pub fn trust_device(&self, mount_point: &str, timestamp: u64) -> Result<(), MediaError> {
        let mount = self
            .mounts
            .read()
            .get(mount_point)
            .cloned()
            .ok_or(MediaError::NotFound)?;

        match &mount.device_id {
            DeviceId::Usb(usb_id) => {
                let device = self.usb_devices.read();
                let name = device
                    .values()
                    .find(|d| d.id() == *usb_id)
                    .map(|d| d.display_name())
                    .unwrap_or_else(|| String::from("USB Device"));
                self.trust_db.trust_usb(usb_id, name, timestamp)
            }
            DeviceId::Network(url) => {
                let name = url.to_url();
                self.trust_db.trust_share(url, name, timestamp)
            }
            DeviceId::Block(_path) => {
                // Block devices can't be trusted by ID easily
                Err(MediaError::InvalidOperation)
            }
        }
    }

    /// Get active mounts
    pub fn list_mounts(&self) -> Vec<ActiveMount> {
        self.mounts.read().values().cloned().collect()
    }

    /// Get known USB devices
    pub fn list_usb_devices(&self) -> Vec<UsbDevice> {
        self.usb_devices.read().values().cloned().collect()
    }

    /// Cleanup stale mounts
    pub fn cleanup_stale(&self, current_time: u64) {
        let policy = self.policy.read();
        if let Some(timeout) = policy.auto_eject_minutes {
            let stale: Vec<String> = self
                .mounts
                .read()
                .iter()
                .filter(|(_, m)| m.is_stale(current_time, timeout))
                .map(|(k, _)| k.clone())
                .collect();

            for mount_point in stale {
                let _ = self.unmount(&mount_point);
            }
        }
    }

    /// Get mount info
    pub fn get_mount(&self, mount_point: &str) -> Option<ActiveMount> {
        self.mounts.read().get(mount_point).cloned()
    }

    /// Update mount activity
    pub fn touch_mount(&self, mount_point: &str, timestamp: u64) -> Result<(), MediaError> {
        let mut mounts = self.mounts.write();
        let mount = mounts.get_mut(mount_point).ok_or(MediaError::NotFound)?;
        mount.touch(timestamp);
        Ok(())
    }
}

impl UsbMonitor for MediaManager {
    fn on_device_connected(&self, device: &UsbDevice) {
        let event = UsbEvent::new(UsbEventType::Connected, device.clone(), 0);
        let _ = self.handle_usb_event(event);
    }

    fn on_device_disconnected(&self, device: &UsbDevice) {
        let _ = self.on_device_disconnected(&device.path);
    }
}
