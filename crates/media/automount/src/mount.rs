//! Mount operations

use alloc::string::String;
use media::{MediaError, MountMode, MountOptions};

/// Mount operation result
pub type MountResult = Result<MountInfo, MediaError>;

/// Mount information
#[derive(Debug, Clone)]
pub struct MountInfo {
    /// Source device or share
    pub source: String,
    /// Mount point
    pub mount_point: String,
    /// Filesystem type
    pub fs_type: String,
    /// Mount mode
    pub mode: MountMode,
    /// Mount time
    pub mounted_at: u64,
}

impl MountInfo {
    /// Create new mount info
    pub fn new(
        source: String,
        mount_point: String,
        fs_type: String,
        mode: MountMode,
        timestamp: u64,
    ) -> Self {
        MountInfo {
            source,
            mount_point,
            fs_type,
            mode,
            mounted_at: timestamp,
        }
    }
}

/// Mount executor trait
pub trait MountExecutor: Send + Sync {
    /// Mount a device
    fn mount(
        &self,
        source: &str,
        mount_point: &str,
        fs_type: &str,
        options: &MountOptions,
    ) -> MountResult;

    /// Unmount a mount point
    fn unmount(&self, mount_point: &str) -> Result<(), MediaError>;

    /// Remount with new options
    fn remount(&self, mount_point: &str, options: &MountOptions) -> Result<(), MediaError>;

    /// Check if mounted
    fn is_mounted(&self, mount_point: &str) -> bool;

    /// Sync filesystem
    fn sync(&self, mount_point: &str) -> Result<(), MediaError>;
}

/// Stub mount executor for no_std
pub struct StubMountExecutor;

impl MountExecutor for StubMountExecutor {
    fn mount(
        &self,
        source: &str,
        mount_point: &str,
        fs_type: &str,
        options: &MountOptions,
    ) -> MountResult {
        Ok(MountInfo::new(
            String::from(source),
            String::from(mount_point),
            String::from(fs_type),
            options.mode,
            0,
        ))
    }

    fn unmount(&self, _mount_point: &str) -> Result<(), MediaError> {
        Ok(())
    }

    fn remount(&self, _mount_point: &str, _options: &MountOptions) -> Result<(), MediaError> {
        Ok(())
    }

    fn is_mounted(&self, _mount_point: &str) -> bool {
        false
    }

    fn sync(&self, _mount_point: &str) -> Result<(), MediaError> {
        Ok(())
    }
}

/// Detect filesystem type from device
pub fn detect_filesystem(device: &[u8]) -> Option<&'static str> {
    if device.len() < 512 {
        return None;
    }

    // Check for FAT32
    if device.len() >= 512 {
        let boot = &device[0..512];
        // FAT32 signature
        if boot[510] == 0x55 && boot[511] == 0xAA {
            // Check for FAT32 marker
            if &boot[82..90] == b"FAT32   " {
                return Some("vfat");
            }
            // Check for FAT16
            if &boot[54..62] == b"FAT16   " || &boot[54..62] == b"FAT12   " {
                return Some("vfat");
            }
        }
    }

    // Check for NTFS
    if device.len() >= 8 && &device[3..7] == b"NTFS" {
        return Some("ntfs");
    }

    // Check for ext4/ext3/ext2
    if device.len() >= 1080 {
        let magic = u16::from_le_bytes([device[1080], device[1081]]);
        if magic == 0xEF53 {
            // Check features to distinguish ext2/3/4
            let compat =
                u32::from_le_bytes([device[1116], device[1117], device[1118], device[1119]]);
            if compat & 0x40 != 0 {
                return Some("ext4");
            }
            let incompat =
                u32::from_le_bytes([device[1120], device[1121], device[1122], device[1123]]);
            if incompat & 0x04 != 0 {
                return Some("ext3");
            }
            return Some("ext2");
        }
    }

    // Check for exFAT
    if device.len() >= 11 && &device[3..11] == b"EXFAT   " {
        return Some("exfat");
    }

    // Check for ISO9660
    if device.len() >= 32769 + 5 && &device[32769..32774] == b"CD001" {
        return Some("iso9660");
    }

    None
}

/// Build mount options string for filesystem
pub fn build_options_string(options: &MountOptions) -> String {
    let mut parts = alloc::vec::Vec::new();

    if options.mode == MountMode::ReadOnly {
        parts.push(String::from("ro"));
    } else {
        parts.push(String::from("rw"));
    }

    if options.noexec {
        parts.push(String::from("noexec"));
    }
    if options.nosuid {
        parts.push(String::from("nosuid"));
    }
    if options.nodev {
        parts.push(String::from("nodev"));
    }
    if options.sync {
        parts.push(String::from("sync"));
    }

    if let Some(uid) = options.uid {
        parts.push(alloc::format!("uid={}", uid));
    }
    if let Some(gid) = options.gid {
        parts.push(alloc::format!("gid={}", gid));
    }
    if let Some(fmask) = options.fmask {
        parts.push(alloc::format!("fmask={:04o}", fmask));
    }
    if let Some(dmask) = options.dmask {
        parts.push(alloc::format!("dmask={:04o}", dmask));
    }
    if let Some(ref charset) = options.charset {
        parts.push(alloc::format!("charset={}", charset));
    }

    let mut result = String::new();
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            result.push(',');
        }
        result.push_str(part);
    }
    result
}
