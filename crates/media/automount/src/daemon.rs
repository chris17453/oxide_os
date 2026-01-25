//! Automount daemon

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::RwLock;

use media::{
    ActiveMount, DeviceTrustDb, MediaError, MediaManager, MediaPolicy, MountOptions, UsbDevice, UsbEvent, UsbEventType,
};

use crate::config::AutomountConfig;
use crate::mount::{MountExecutor, MountInfo, StubMountExecutor};

/// Automount daemon state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonState {
    /// Not started
    Stopped,
    /// Running
    Running,
    /// Paused
    Paused,
}

/// Authentication token for promotion
#[derive(Debug, Clone)]
pub struct AuthToken {
    /// User ID
    pub uid: u32,
    /// Token data
    pub token: [u8; 32],
    /// Expiry timestamp
    pub expires: u64,
}

impl AuthToken {
    /// Create new auth token
    pub fn new(uid: u32, token: [u8; 32], expires: u64) -> Self {
        AuthToken {
            uid,
            token,
            expires,
        }
    }

    /// Check if token is expired
    pub fn is_expired(&self, current_time: u64) -> bool {
        current_time > self.expires
    }
}

/// Automount daemon
pub struct AutomountDaemon {
    /// Media manager
    media: Arc<MediaManager>,
    /// Configuration
    config: RwLock<AutomountConfig>,
    /// Mount executor
    executor: Arc<dyn MountExecutor>,
    /// Daemon state
    state: RwLock<DaemonState>,
    /// Pending promotion requests
    pending_promotions: RwLock<BTreeMap<String, u64>>,
    /// Last cleanup time
    last_cleanup: RwLock<u64>,
}

impl AutomountDaemon {
    /// Create new daemon with default executor
    pub fn new(trust_db: Arc<DeviceTrustDb>) -> Self {
        Self::with_executor(trust_db, Arc::new(StubMountExecutor))
    }

    /// Create new daemon with custom executor
    pub fn with_executor(trust_db: Arc<DeviceTrustDb>, executor: Arc<dyn MountExecutor>) -> Self {
        let media = Arc::new(MediaManager::new(trust_db));

        AutomountDaemon {
            media,
            config: RwLock::new(AutomountConfig::new()),
            executor,
            state: RwLock::new(DaemonState::Stopped),
            pending_promotions: RwLock::new(BTreeMap::new()),
            last_cleanup: RwLock::new(0),
        }
    }

    /// Set configuration
    pub fn set_config(&self, config: AutomountConfig) {
        let policy = MediaPolicy {
            default_mount: config.default_mode,
            require_auth_for_rw: config.require_auth_for_rw,
            auto_eject_minutes: if config.auto_eject_minutes > 0 {
                Some(config.auto_eject_minutes)
            } else {
                None
            },
            verify_executables: true,
            allow_autorun: false,
            max_mount_time: 0,
            log_mounts: config.log_operations,
            quarantine_executables: true,
        };

        self.media.set_policy(policy);
        self.media.set_mount_base(&config.mount_base);
        *self.config.write() = config;
    }

    /// Get configuration
    pub fn config(&self) -> AutomountConfig {
        self.config.read().clone()
    }

    /// Start daemon
    pub fn start(&self) -> Result<(), MediaError> {
        let mut state = self.state.write();
        if *state == DaemonState::Running {
            return Ok(());
        }
        *state = DaemonState::Running;
        Ok(())
    }

    /// Stop daemon
    pub fn stop(&self) -> Result<(), MediaError> {
        let mut state = self.state.write();
        *state = DaemonState::Stopped;
        Ok(())
    }

    /// Pause daemon
    pub fn pause(&self) -> Result<(), MediaError> {
        let mut state = self.state.write();
        if *state != DaemonState::Running {
            return Err(MediaError::InvalidOperation);
        }
        *state = DaemonState::Paused;
        Ok(())
    }

    /// Resume daemon
    pub fn resume(&self) -> Result<(), MediaError> {
        let mut state = self.state.write();
        if *state != DaemonState::Paused {
            return Err(MediaError::InvalidOperation);
        }
        *state = DaemonState::Running;
        Ok(())
    }

    /// Get daemon state
    pub fn state(&self) -> DaemonState {
        *self.state.read()
    }

    /// Handle device connected event
    pub fn on_device_connected(
        &self,
        device: UsbDevice,
        timestamp: u64,
    ) -> Result<MountInfo, MediaError> {
        if *self.state.read() != DaemonState::Running {
            return Err(MediaError::InvalidOperation);
        }

        let config = self.config.read();
        if !config.enable_usb {
            return Err(MediaError::PermissionDenied);
        }

        // Let media manager handle the event
        let event = UsbEvent::new(UsbEventType::Connected, device.clone(), timestamp);
        self.media.handle_usb_event(event)?;

        // Get mount info
        let mounts = self.media.list_mounts();
        let mount = mounts.last().ok_or(MediaError::MountFailed)?;

        Ok(MountInfo::new(
            device.path.clone(),
            mount.mount_point.clone(),
            String::from("auto"),
            mount.mode,
            timestamp,
        ))
    }

    /// Handle device disconnected event
    pub fn on_device_disconnected(&self, _device_path: &str) -> Result<(), MediaError> {
        // Find mount for this device
        let mounts = self.media.list_mounts();
        for mount in mounts {
            // Unmount
            self.executor.unmount(&mount.mount_point)?;
        }

        // Let media manager handle cleanup
        let device = UsbDevice::new(0, 0);
        let event = UsbEvent {
            event_type: UsbEventType::Disconnected,
            device,
            timestamp: 0,
        };
        self.media.handle_usb_event(event)?;

        Ok(())
    }

    /// Handle promotion request
    pub fn handle_promotion(
        &self,
        mount_point: &str,
        auth: &AuthToken,
        timestamp: u64,
    ) -> Result<(), MediaError> {
        // Check auth token
        if auth.is_expired(timestamp) {
            return Err(MediaError::AuthRequired);
        }

        // Request promotion through media manager
        let needs_auth = !self.media.request_promotion(mount_point)?;

        if needs_auth && self.config.read().require_auth_for_rw {
            // Verify auth
            // In real implementation, validate the token
        }

        // Grant promotion
        self.media.grant_promotion(mount_point, timestamp)?;

        // Remount as read-write
        let options = MountOptions::read_write();
        self.executor.remount(mount_point, &options)?;

        // Remove from pending
        self.pending_promotions.write().remove(mount_point);

        Ok(())
    }

    /// Periodic cleanup
    pub fn cleanup(&self, current_time: u64) {
        let config = self.config.read();

        // Check if cleanup is due
        let last = *self.last_cleanup.read();
        if current_time < last + config.cleanup_interval as u64 {
            return;
        }

        *self.last_cleanup.write() = current_time;

        // Cleanup stale mounts
        self.media.cleanup_stale(current_time);

        // Cleanup expired promotion requests
        let mut pending = self.pending_promotions.write();
        let timeout = 300u64; // 5 minute timeout for promotion requests
        pending.retain(|_, &mut requested_at| current_time < requested_at + timeout);
    }

    /// Trust device at mount point
    pub fn trust_device(&self, mount_point: &str, timestamp: u64) -> Result<(), MediaError> {
        self.media.trust_device(mount_point, timestamp)
    }

    /// Eject device
    pub fn eject(&self, mount_point: &str) -> Result<(), MediaError> {
        // Sync filesystem
        self.executor.sync(mount_point)?;

        // Unmount
        self.executor.unmount(mount_point)?;

        // Remove from media manager
        self.media.unmount(mount_point)?;

        Ok(())
    }

    /// Get active mounts
    pub fn list_mounts(&self) -> Vec<ActiveMount> {
        self.media.list_mounts()
    }

    /// Get mount info
    pub fn get_mount(&self, mount_point: &str) -> Option<ActiveMount> {
        self.media.get_mount(mount_point)
    }

    /// Get media manager
    pub fn media_manager(&self) -> &Arc<MediaManager> {
        &self.media
    }
}
