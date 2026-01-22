//! Media mount policies

use crate::{MountMode, TrustLevel};

/// Media mount policy
#[derive(Debug, Clone)]
pub struct MediaPolicy {
    /// Default mount mode for unknown devices
    pub default_mount: MountMode,
    /// Require authentication for read-write promotion
    pub require_auth_for_rw: bool,
    /// Auto-eject after inactivity (minutes)
    pub auto_eject_minutes: Option<u32>,
    /// Verify signatures on executables
    pub verify_executables: bool,
    /// Allow autorun scripts (dangerous!)
    pub allow_autorun: bool,
    /// Maximum mount time (minutes, 0 = unlimited)
    pub max_mount_time: u32,
    /// Log all mount operations
    pub log_mounts: bool,
    /// Quarantine unknown executables
    pub quarantine_executables: bool,
}

impl MediaPolicy {
    /// Create new policy with defaults
    pub fn new() -> Self {
        MediaPolicy {
            default_mount: MountMode::ReadOnly,
            require_auth_for_rw: true,
            auto_eject_minutes: Some(30),
            verify_executables: true,
            allow_autorun: false,
            max_mount_time: 0,
            log_mounts: true,
            quarantine_executables: true,
        }
    }

    /// Create permissive policy (use with caution)
    pub fn permissive() -> Self {
        MediaPolicy {
            default_mount: MountMode::ReadWrite,
            require_auth_for_rw: false,
            auto_eject_minutes: None,
            verify_executables: false,
            allow_autorun: false,
            max_mount_time: 0,
            log_mounts: true,
            quarantine_executables: false,
        }
    }

    /// Create strict policy
    pub fn strict() -> Self {
        MediaPolicy {
            default_mount: MountMode::ReadOnly,
            require_auth_for_rw: true,
            auto_eject_minutes: Some(15),
            verify_executables: true,
            allow_autorun: false,
            max_mount_time: 60,
            log_mounts: true,
            quarantine_executables: true,
        }
    }

    /// Determine mount mode for trust level
    pub fn mount_mode_for_trust(&self, trust: TrustLevel) -> MountMode {
        match trust {
            TrustLevel::Trusted => MountMode::ReadWrite,
            TrustLevel::AskOnConnect => self.default_mount,
            TrustLevel::ReadOnly => MountMode::ReadOnly,
            TrustLevel::Blocked => MountMode::ReadOnly, // Should not reach here
        }
    }

    /// Check if mount is allowed
    pub fn allows_mount(&self, trust: TrustLevel) -> bool {
        trust != TrustLevel::Blocked
    }

    /// Check if promotion to RW is allowed
    pub fn allows_promotion(&self, trust: TrustLevel) -> bool {
        trust != TrustLevel::Blocked && trust != TrustLevel::ReadOnly
    }
}

impl Default for MediaPolicy {
    fn default() -> Self {
        Self::new()
    }
}

/// Mount options
#[derive(Debug, Clone)]
pub struct MountOptions {
    /// Mount mode
    pub mode: MountMode,
    /// Filesystem type override
    pub fs_type: Option<alloc::string::String>,
    /// No-exec flag
    pub noexec: bool,
    /// No-suid flag
    pub nosuid: bool,
    /// No-dev flag
    pub nodev: bool,
    /// Synchronous I/O
    pub sync: bool,
    /// Mount read-only even if RW requested (fallback)
    pub fallback_ro: bool,
    /// User ID for files
    pub uid: Option<u32>,
    /// Group ID for files
    pub gid: Option<u32>,
    /// File mode mask
    pub fmask: Option<u16>,
    /// Directory mode mask
    pub dmask: Option<u16>,
    /// Character encoding
    pub charset: Option<alloc::string::String>,
}

impl MountOptions {
    /// Create new mount options
    pub fn new(mode: MountMode) -> Self {
        MountOptions {
            mode,
            fs_type: None,
            noexec: true, // Safe default
            nosuid: true, // Safe default
            nodev: true,  // Safe default
            sync: false,
            fallback_ro: true,
            uid: None,
            gid: None,
            fmask: None,
            dmask: None,
            charset: None,
        }
    }

    /// Create options for read-only mount
    pub fn read_only() -> Self {
        Self::new(MountMode::ReadOnly)
    }

    /// Create options for read-write mount
    pub fn read_write() -> Self {
        Self::new(MountMode::ReadWrite)
    }

    /// Set filesystem type
    pub fn with_fs_type(mut self, fs_type: &str) -> Self {
        self.fs_type = Some(alloc::string::String::from(fs_type));
        self
    }

    /// Allow exec
    pub fn allow_exec(mut self) -> Self {
        self.noexec = false;
        self
    }

    /// Set owner
    pub fn with_owner(mut self, uid: u32, gid: u32) -> Self {
        self.uid = Some(uid);
        self.gid = Some(gid);
        self
    }

    /// Set file mask
    pub fn with_fmask(mut self, mask: u16) -> Self {
        self.fmask = Some(mask);
        self
    }

    /// Set directory mask
    pub fn with_dmask(mut self, mask: u16) -> Self {
        self.dmask = Some(mask);
        self
    }
}

impl Default for MountOptions {
    fn default() -> Self {
        Self::read_only()
    }
}
