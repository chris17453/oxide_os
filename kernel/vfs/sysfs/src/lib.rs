//! System filesystem (sysfs) for OXIDE OS
//!
//! Provides /sys with kernel object hierarchy.
//!
//! Structure:
//! - /sys/kernel/       - kernel subsystem info
//! - /sys/kernel/debug/ - debug interfaces
//! - /sys/devices/      - device tree (future)
//! - /sys/class/        - device classes (future)
//! - /sys/bus/          - bus types (future)
//! - /sys/firmware/     - firmware interfaces (future)
//!
//! -- IronGhost: Exposing the machine's skeleton through /sys
//! -- NightDoc: Minimal sysfs scaffolding for userspace tool compat

#![no_std]
#![allow(unused)]

extern crate alloc;

use alloc::string::String;
use alloc::sync::Arc;
use vfs::{DirEntry, Mode, Stat, VfsError, VfsResult, VnodeOps, VnodeType};

// ============================================================================
// Root: /sys
// ============================================================================

/// The /sys root directory
/// -- IronGhost: Top-level sysfs node, gateway to the hardware truth
pub struct SysFs {
    ino: u64,
}

impl SysFs {
    /// Create a new sysfs root
    pub fn new() -> Arc<Self> {
        Arc::new(SysFs { ino: 1 })
    }
}

/// Static directory entries for /sys root
const SYS_ENTRIES: &[(&str, u64, VnodeType)] = &[
    ("kernel", 10, VnodeType::Directory),
    ("devices", 20, VnodeType::Directory),
    ("class", 30, VnodeType::Directory),
    ("bus", 40, VnodeType::Directory),
    ("firmware", 50, VnodeType::Directory),
];

impl VnodeOps for SysFs {
    fn vtype(&self) -> VnodeType {
        VnodeType::Directory
    }

    fn lookup(&self, name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        match name {
            "kernel" => Ok(Arc::new(SysKernel { ino: 10 })),
            "devices" => Ok(Arc::new(SysEmpty { ino: 20, name: "devices" })),
            "class" => Ok(Arc::new(SysEmpty { ino: 30, name: "class" })),
            "bus" => Ok(Arc::new(SysEmpty { ino: 40, name: "bus" })),
            "firmware" => Ok(Arc::new(SysEmpty { ino: 50, name: "firmware" })),
            _ => Err(VfsError::NotFound),
        }
    }

    fn readdir(&self, offset: u64) -> VfsResult<Option<DirEntry>> {
        let idx = offset as usize;

        // . and ..
        match idx {
            0 => return Ok(Some(DirEntry {
                name: String::from("."),
                ino: self.ino,
                file_type: VnodeType::Directory,
            })),
            1 => return Ok(Some(DirEntry {
                name: String::from(".."),
                ino: self.ino,
                file_type: VnodeType::Directory,
            })),
            _ => {}
        }

        let entry_idx = idx - 2;
        if entry_idx < SYS_ENTRIES.len() {
            let (name, ino, ftype) = SYS_ENTRIES[entry_idx];
            Ok(Some(DirEntry {
                name: String::from(name),
                ino,
                file_type: ftype,
            }))
        } else {
            Ok(None)
        }
    }

    fn stat(&self) -> VfsResult<Stat> {
        Ok(Stat::new(VnodeType::Directory, Mode::new(0o555), 0, self.ino))
    }

    fn read(&self, _offset: u64, _buf: &mut [u8]) -> VfsResult<usize> {
        Err(VfsError::IsDirectory)
    }

    fn write(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::IsDirectory)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::ReadOnly)
    }

    fn mkdir(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::ReadOnly)
    }

    fn rmdir(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn unlink(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn rename(&self, _old: &str, _new_dir: &dyn VnodeOps, _new: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }
}

// ============================================================================
// /sys/kernel
// ============================================================================

/// The /sys/kernel directory
/// -- IronGhost: Kernel internals exposed to the light
struct SysKernel {
    ino: u64,
}

const KERNEL_ENTRIES: &[(&str, u64, VnodeType)] = &[
    ("debug", 11, VnodeType::Directory),
];

impl VnodeOps for SysKernel {
    fn vtype(&self) -> VnodeType {
        VnodeType::Directory
    }

    fn lookup(&self, name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        match name {
            "debug" => Ok(Arc::new(SysEmpty { ino: 11, name: "debug" })),
            _ => Err(VfsError::NotFound),
        }
    }

    fn readdir(&self, offset: u64) -> VfsResult<Option<DirEntry>> {
        let idx = offset as usize;
        match idx {
            0 => Ok(Some(DirEntry {
                name: String::from("."),
                ino: self.ino,
                file_type: VnodeType::Directory,
            })),
            1 => Ok(Some(DirEntry {
                name: String::from(".."),
                ino: 1, // parent is /sys
                file_type: VnodeType::Directory,
            })),
            _ => {
                let entry_idx = idx - 2;
                if entry_idx < KERNEL_ENTRIES.len() {
                    let (name, ino, ftype) = KERNEL_ENTRIES[entry_idx];
                    Ok(Some(DirEntry {
                        name: String::from(name),
                        ino,
                        file_type: ftype,
                    }))
                } else {
                    Ok(None)
                }
            }
        }
    }

    fn stat(&self) -> VfsResult<Stat> {
        Ok(Stat::new(VnodeType::Directory, Mode::new(0o555), 0, self.ino))
    }

    fn read(&self, _offset: u64, _buf: &mut [u8]) -> VfsResult<usize> {
        Err(VfsError::IsDirectory)
    }

    fn write(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::IsDirectory)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::ReadOnly)
    }

    fn mkdir(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::ReadOnly)
    }

    fn rmdir(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn unlink(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn rename(&self, _old: &str, _new_dir: &dyn VnodeOps, _new: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }
}

// ============================================================================
// Empty placeholder directories
// ============================================================================

/// Empty sysfs directory (placeholder for future subsystems)
/// -- NightDoc: Stub directories so ls /sys doesn't lie about what exists
struct SysEmpty {
    ino: u64,
    name: &'static str,
}

impl VnodeOps for SysEmpty {
    fn vtype(&self) -> VnodeType {
        VnodeType::Directory
    }

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotFound)
    }

    fn readdir(&self, offset: u64) -> VfsResult<Option<DirEntry>> {
        match offset as usize {
            0 => Ok(Some(DirEntry {
                name: String::from("."),
                ino: self.ino,
                file_type: VnodeType::Directory,
            })),
            1 => Ok(Some(DirEntry {
                name: String::from(".."),
                ino: 1,
                file_type: VnodeType::Directory,
            })),
            _ => Ok(None),
        }
    }

    fn stat(&self) -> VfsResult<Stat> {
        Ok(Stat::new(VnodeType::Directory, Mode::new(0o555), 0, self.ino))
    }

    fn read(&self, _offset: u64, _buf: &mut [u8]) -> VfsResult<usize> {
        Err(VfsError::IsDirectory)
    }

    fn write(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::IsDirectory)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::ReadOnly)
    }

    fn mkdir(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::ReadOnly)
    }

    fn rmdir(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn unlink(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn rename(&self, _old: &str, _new_dir: &dyn VnodeOps, _new: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }
}
