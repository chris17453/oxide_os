//! Automount configuration

use alloc::string::String;
use media::MountMode;

/// Automount configuration
#[derive(Debug, Clone)]
pub struct AutomountConfig {
    /// Base path for mounts
    pub mount_base: String,
    /// Default mount mode
    pub default_mode: MountMode,
    /// Require authentication for RW promotion
    pub require_auth_for_rw: bool,
    /// Auto-eject after inactivity (minutes, 0 = disabled)
    pub auto_eject_minutes: u32,
    /// Enable USB automounting
    pub enable_usb: bool,
    /// Enable network share discovery
    pub enable_network: bool,
    /// Poll interval for cleanup (seconds)
    pub cleanup_interval: u32,
    /// Log mount operations
    pub log_operations: bool,
    /// Notify on mount
    pub notify_on_mount: bool,
    /// Mount options for FAT filesystems
    pub fat_options: String,
    /// Mount options for NTFS filesystems
    pub ntfs_options: String,
    /// Mount options for ext4 filesystems
    pub ext4_options: String,
}

impl AutomountConfig {
    /// Create new config with defaults
    pub fn new() -> Self {
        AutomountConfig {
            mount_base: String::from("/media"),
            default_mode: MountMode::ReadOnly,
            require_auth_for_rw: true,
            auto_eject_minutes: 30,
            enable_usb: true,
            enable_network: false,
            cleanup_interval: 60,
            log_operations: true,
            notify_on_mount: true,
            fat_options: String::from("noexec,nosuid,nodev,utf8"),
            ntfs_options: String::from("noexec,nosuid,nodev,utf8"),
            ext4_options: String::from("noexec,nosuid,nodev"),
        }
    }

    /// Create strict config
    pub fn strict() -> Self {
        AutomountConfig {
            mount_base: String::from("/media"),
            default_mode: MountMode::ReadOnly,
            require_auth_for_rw: true,
            auto_eject_minutes: 15,
            enable_usb: true,
            enable_network: false,
            cleanup_interval: 30,
            log_operations: true,
            notify_on_mount: true,
            fat_options: String::from("noexec,nosuid,nodev,utf8,ro"),
            ntfs_options: String::from("noexec,nosuid,nodev,utf8,ro"),
            ext4_options: String::from("noexec,nosuid,nodev,ro"),
        }
    }

    /// Create permissive config
    pub fn permissive() -> Self {
        AutomountConfig {
            mount_base: String::from("/media"),
            default_mode: MountMode::ReadWrite,
            require_auth_for_rw: false,
            auto_eject_minutes: 0,
            enable_usb: true,
            enable_network: true,
            cleanup_interval: 300,
            log_operations: true,
            notify_on_mount: true,
            fat_options: String::from("nosuid,nodev,utf8"),
            ntfs_options: String::from("nosuid,nodev,utf8"),
            ext4_options: String::from("nosuid,nodev"),
        }
    }

    /// Parse from config file content
    pub fn parse(content: &str) -> Self {
        let mut config = Self::new();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
                continue;
            }

            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim();

                match key {
                    "MountBase" => config.mount_base = String::from(value),
                    "DefaultMode" => {
                        config.default_mode = if value == "rw" {
                            MountMode::ReadWrite
                        } else {
                            MountMode::ReadOnly
                        };
                    }
                    "RequireAuthForRW" => {
                        config.require_auth_for_rw = value == "true" || value == "1";
                    }
                    "AutoEject" => {
                        config.auto_eject_minutes = value.parse().unwrap_or(30);
                    }
                    "EnableUSB" => {
                        config.enable_usb = value == "true" || value == "1";
                    }
                    "EnableNetwork" => {
                        config.enable_network = value == "true" || value == "1";
                    }
                    "CleanupInterval" => {
                        config.cleanup_interval = value.parse().unwrap_or(60);
                    }
                    "LogOperations" => {
                        config.log_operations = value == "true" || value == "1";
                    }
                    "NotifyOnMount" => {
                        config.notify_on_mount = value == "true" || value == "1";
                    }
                    _ => {}
                }
            }
        }

        config
    }

    /// Serialize to config file content
    pub fn to_config(&self) -> String {
        alloc::format!(
            "[Automount]\n\
             MountBase={}\n\
             DefaultMode={}\n\
             RequireAuthForRW={}\n\
             AutoEject={}\n\
             EnableUSB={}\n\
             EnableNetwork={}\n\
             CleanupInterval={}\n\
             LogOperations={}\n\
             NotifyOnMount={}\n",
            self.mount_base,
            if self.default_mode == MountMode::ReadWrite {
                "rw"
            } else {
                "ro"
            },
            self.require_auth_for_rw,
            self.auto_eject_minutes,
            self.enable_usb,
            self.enable_network,
            self.cleanup_interval,
            self.log_operations,
            self.notify_on_mount,
        )
    }
}

impl Default for AutomountConfig {
    fn default() -> Self {
        Self::new()
    }
}
