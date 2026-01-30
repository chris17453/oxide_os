//! Device filesystem (devfs) for OXIDE OS
//!
//! Provides virtual device files like /dev/null, /dev/zero, /dev/console.

#![no_std]
#![allow(unused)]

extern crate alloc;

pub mod devices;
pub mod kmsg;

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use spin::RwLock;

use vfs::{DirEntry, Mode, Stat, VfsError, VfsResult, VnodeOps, VnodeType};

use devices::{ConsoleDevice, FramebufferDevice, NullDevice, RandomDevice, ZeroDevice};
use kmsg::KmsgDevice;

// Re-export console input functions
pub use devices::{console_has_input, console_push_char, console_push_str, set_console_blocked_reader};

// Re-export signal callback setter for Ctrl+C handling
pub use devices::{SIGINT, SIGQUIT, set_signal_fg_callback};

/// Console input callback for PS/2 driver
/// Takes a slice of bytes from the keyboard and pushes them to console input
pub fn console_input_callback(data: &[u8]) {
    for &byte in data {
        console_push_char(byte);
    }
}

// Re-export random device callback setter
pub use devices::set_random_fill_callback;

// Re-export kmsg functions and callback setters
pub use kmsg::{kmsg_write, kmsg_write_str, set_pid_callback, set_uptime_callback, set_proc_name_callback};

/// The devfs root directory
pub struct DevFs {
    /// Registered devices
    devices: RwLock<BTreeMap<String, Arc<dyn VnodeOps>>>,
    /// Inode number
    ino: u64,
}

impl DevFs {
    /// Create a new devfs with default devices
    pub fn new() -> Arc<Self> {
        let devfs = Arc::new(DevFs {
            devices: RwLock::new(BTreeMap::new()),
            ino: 1,
        });

        // Register default devices
        {
            let mut devices = devfs.devices.write();
            devices.insert("null".to_string(), Arc::new(NullDevice::new(2)));
            devices.insert("zero".to_string(), Arc::new(ZeroDevice::new(3)));
            devices.insert("console".to_string(), Arc::new(ConsoleDevice::new(4)));
            devices.insert("fb0".to_string(), Arc::new(FramebufferDevice::new(5)));
            devices.insert(
                "urandom".to_string(),
                Arc::new(RandomDevice::new_urandom(6)),
            );
            devices.insert("random".to_string(), Arc::new(RandomDevice::new_random(7)));
            devices.insert("kmsg".to_string(), Arc::new(KmsgDevice::new(8)));
        }

        devfs
    }

    /// Register a new device
    pub fn register(&self, name: &str, device: Arc<dyn VnodeOps>) {
        let mut devices = self.devices.write();
        devices.insert(name.to_string(), device);
    }

    /// Unregister a device
    pub fn unregister(&self, name: &str) -> VfsResult<()> {
        let mut devices = self.devices.write();
        devices.remove(name).ok_or(VfsError::NotFound)?;
        Ok(())
    }
}

impl Default for DevFs {
    fn default() -> Self {
        DevFs {
            devices: RwLock::new(BTreeMap::new()),
            ino: 1,
        }
    }
}

impl VnodeOps for DevFs {
    fn vtype(&self) -> VnodeType {
        VnodeType::Directory
    }

    fn lookup(&self, name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        let devices = self.devices.read();
        devices.get(name).cloned().ok_or(VfsError::NotFound)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        // Can't create files in devfs - devices must be registered
        Err(VfsError::PermissionDenied)
    }

    fn read(&self, _offset: u64, _buf: &mut [u8]) -> VfsResult<usize> {
        Err(VfsError::IsDirectory)
    }

    fn write(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::IsDirectory)
    }

    fn readdir(&self, offset: u64) -> VfsResult<Option<DirEntry>> {
        let devices = self.devices.read();
        let entries: alloc::vec::Vec<_> = devices.iter().collect();

        let offset = offset as usize;

        // First two entries are . and ..
        if offset == 0 {
            return Ok(Some(DirEntry {
                name: ".".to_string(),
                ino: self.ino,
                file_type: VnodeType::Directory,
            }));
        }

        if offset == 1 {
            return Ok(Some(DirEntry {
                name: "..".to_string(),
                ino: self.ino,
                file_type: VnodeType::Directory,
            }));
        }

        // Device entries start at offset 2
        let device_idx = offset - 2;
        if device_idx < entries.len() {
            let (name, vnode) = &entries[device_idx];
            return Ok(Some(DirEntry {
                name: (*name).clone(),
                ino: vnode.stat().map(|s| s.ino).unwrap_or(0),
                file_type: vnode.vtype(),
            }));
        }

        Ok(None)
    }

    fn mkdir(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::PermissionDenied)
    }

    fn rmdir(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::PermissionDenied)
    }

    fn unlink(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::PermissionDenied)
    }

    fn rename(&self, _old_name: &str, _new_dir: &dyn VnodeOps, _new_name: &str) -> VfsResult<()> {
        Err(VfsError::PermissionDenied)
    }

    fn stat(&self) -> VfsResult<Stat> {
        Ok(Stat::new(
            VnodeType::Directory,
            Mode::DEFAULT_DIR,
            0,
            self.ino,
        ))
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::IsDirectory)
    }
}
