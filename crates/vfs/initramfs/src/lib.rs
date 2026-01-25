//! Initramfs filesystem for OXIDE OS
//!
//! Provides a read-only filesystem loaded from a CPIO archive at boot time.

#![no_std]
#![allow(unused)]

extern crate alloc;

pub mod cpio;

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::RwLock;

use vfs::{DirEntry, Mode, Stat, VfsError, VfsResult, VnodeOps, VnodeType};

use cpio::CpioError;

/// Inode allocator for initramfs
static INITRAMFS_INODE: AtomicU64 = AtomicU64::new(1);

fn alloc_inode() -> u64 {
    INITRAMFS_INODE.fetch_add(1, Ordering::Relaxed)
}

/// Convert CPIO mode to VnodeType
fn mode_to_vtype(mode: u32) -> VnodeType {
    match mode & 0o170000 {
        0o100000 => VnodeType::File,
        0o040000 => VnodeType::Directory,
        0o120000 => VnodeType::Symlink,
        0o020000 => VnodeType::CharDevice,
        0o060000 => VnodeType::BlockDevice,
        0o010000 => VnodeType::Fifo,
        0o140000 => VnodeType::Socket,
        _ => VnodeType::File,
    }
}

/// An entry in the initramfs
enum InitramfsEntry {
    File(Arc<InitramfsFile>),
    Dir(Arc<InitramfsDir>),
    Symlink(Arc<InitramfsSymlink>),
    Device(Arc<InitramfsDevice>),
}

impl InitramfsEntry {
    fn as_vnode(&self) -> Arc<dyn VnodeOps> {
        match self {
            InitramfsEntry::File(f) => f.clone(),
            InitramfsEntry::Dir(d) => d.clone(),
            InitramfsEntry::Symlink(s) => s.clone(),
            InitramfsEntry::Device(dev) => dev.clone(),
        }
    }

    fn vtype(&self) -> VnodeType {
        match self {
            InitramfsEntry::File(_) => VnodeType::File,
            InitramfsEntry::Dir(_) => VnodeType::Directory,
            InitramfsEntry::Symlink(_) => VnodeType::Symlink,
            InitramfsEntry::Device(d) => d.vtype,
        }
    }

    fn ino(&self) -> u64 {
        match self {
            InitramfsEntry::File(f) => f.ino,
            InitramfsEntry::Dir(d) => d.ino,
            InitramfsEntry::Symlink(s) => s.ino,
            InitramfsEntry::Device(d) => d.ino,
        }
    }
}

/// A file in the initramfs
pub struct InitramfsFile {
    /// File contents
    data: Vec<u8>,
    /// Inode number
    ino: u64,
    /// File mode
    mode: Mode,
}

impl VnodeOps for InitramfsFile {
    fn vtype(&self) -> VnodeType {
        VnodeType::File
    }

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::ReadOnly)
    }

    fn read(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        let offset = offset as usize;
        if offset >= self.data.len() {
            return Ok(0);
        }

        let available = self.data.len() - offset;
        let to_read = buf.len().min(available);
        buf[..to_read].copy_from_slice(&self.data[offset..offset + to_read]);
        Ok(to_read)
    }

    fn write(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::ReadOnly)
    }

    fn readdir(&self, _offset: u64) -> VfsResult<Option<DirEntry>> {
        Err(VfsError::NotDirectory)
    }

    fn mkdir(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn rmdir(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::NotDirectory)
    }

    fn unlink(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::NotDirectory)
    }

    fn rename(&self, _old_name: &str, _new_dir: &dyn VnodeOps, _new_name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn stat(&self) -> VfsResult<Stat> {
        Ok(Stat::new(
            VnodeType::File,
            self.mode,
            self.data.len() as u64,
            self.ino,
        ))
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn size(&self) -> u64 {
        self.data.len() as u64
    }
}

/// A symbolic link in the initramfs
pub struct InitramfsSymlink {
    /// Link target path
    target: String,
    /// Inode number
    ino: u64,
    /// Link mode (permissions)
    mode: Mode,
}

impl VnodeOps for InitramfsSymlink {
    fn vtype(&self) -> VnodeType {
        VnodeType::Symlink
    }

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::ReadOnly)
    }

    fn read(&self, _offset: u64, _buf: &mut [u8]) -> VfsResult<usize> {
        Err(VfsError::InvalidOperation)
    }

    fn write(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::ReadOnly)
    }

    fn readdir(&self, _offset: u64) -> VfsResult<Option<DirEntry>> {
        Err(VfsError::NotDirectory)
    }

    fn mkdir(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn rmdir(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::NotDirectory)
    }

    fn unlink(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::NotDirectory)
    }

    fn rename(&self, _old_name: &str, _new_dir: &dyn VnodeOps, _new_name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn stat(&self) -> VfsResult<Stat> {
        Ok(Stat::new(
            VnodeType::Symlink,
            self.mode,
            self.target.len() as u64,
            self.ino,
        ))
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn readlink(&self) -> VfsResult<String> {
        Ok(self.target.clone())
    }
}

/// A device node in the initramfs
pub struct InitramfsDevice {
    /// Device type (char or block)
    vtype: VnodeType,
    /// Device major number
    major: u32,
    /// Device minor number
    minor: u32,
    /// Inode number
    ino: u64,
    /// Device mode (permissions)
    mode: Mode,
}

impl VnodeOps for InitramfsDevice {
    fn vtype(&self) -> VnodeType {
        self.vtype
    }

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::ReadOnly)
    }

    fn read(&self, _offset: u64, _buf: &mut [u8]) -> VfsResult<usize> {
        // Device reads should go through the device driver
        Err(VfsError::NotSupported)
    }

    fn write(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        // Device writes should go through the device driver
        Err(VfsError::NotSupported)
    }

    fn readdir(&self, _offset: u64) -> VfsResult<Option<DirEntry>> {
        Err(VfsError::NotDirectory)
    }

    fn mkdir(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn rmdir(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::NotDirectory)
    }

    fn unlink(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::NotDirectory)
    }

    fn rename(&self, _old_name: &str, _new_dir: &dyn VnodeOps, _new_name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn stat(&self) -> VfsResult<Stat> {
        let mut stat = Stat::new(self.vtype, self.mode, 0, self.ino);
        stat.rdev = ((self.major as u64) << 8) | (self.minor as u64 & 0xFF);
        Ok(stat)
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }
}

/// A directory in the initramfs
pub struct InitramfsDir {
    /// Directory entries
    entries: RwLock<BTreeMap<String, InitramfsEntry>>,
    /// Inode number
    ino: u64,
    /// Directory mode
    mode: Mode,
}

impl InitramfsDir {
    /// Create a new empty directory
    fn new(mode: Mode) -> Self {
        InitramfsDir {
            entries: RwLock::new(BTreeMap::new()),
            ino: alloc_inode(),
            mode,
        }
    }

    /// Add an entry to this directory
    fn add_entry(&self, name: &str, entry: InitramfsEntry) {
        self.entries.write().insert(name.to_string(), entry);
    }

    /// Get or create a subdirectory
    fn get_or_create_subdir(&self, name: &str) -> Arc<InitramfsDir> {
        let mut entries = self.entries.write();

        if let Some(InitramfsEntry::Dir(dir)) = entries.get(name) {
            return dir.clone();
        }

        let new_dir = Arc::new(InitramfsDir::new(Mode::DEFAULT_DIR));
        entries.insert(name.to_string(), InitramfsEntry::Dir(new_dir.clone()));
        new_dir
    }
}

impl VnodeOps for InitramfsDir {
    fn vtype(&self) -> VnodeType {
        VnodeType::Directory
    }

    fn lookup(&self, name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        self.entries
            .read()
            .get(name)
            .map(|e| e.as_vnode())
            .ok_or(VfsError::NotFound)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::ReadOnly)
    }

    fn read(&self, _offset: u64, _buf: &mut [u8]) -> VfsResult<usize> {
        Err(VfsError::IsDirectory)
    }

    fn write(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::IsDirectory)
    }

    fn readdir(&self, offset: u64) -> VfsResult<Option<DirEntry>> {
        let entries = self.entries.read();
        let all_entries: Vec<_> = entries.iter().collect();

        let offset = offset as usize;

        // . entry
        if offset == 0 {
            return Ok(Some(DirEntry {
                name: ".".to_string(),
                ino: self.ino,
                file_type: VnodeType::Directory,
            }));
        }

        // .. entry
        if offset == 1 {
            return Ok(Some(DirEntry {
                name: "..".to_string(),
                ino: self.ino,
                file_type: VnodeType::Directory,
            }));
        }

        // Regular entries
        let entry_idx = offset - 2;
        if entry_idx < all_entries.len() {
            let (name, entry) = all_entries[entry_idx];
            return Ok(Some(DirEntry {
                name: name.clone(),
                ino: entry.ino(),
                file_type: entry.vtype(),
            }));
        }

        Ok(None)
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

    fn rename(&self, _old_name: &str, _new_dir: &dyn VnodeOps, _new_name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn stat(&self) -> VfsResult<Stat> {
        Ok(Stat::new(VnodeType::Directory, self.mode, 0, self.ino))
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::IsDirectory)
    }
}

/// Error loading initramfs
#[derive(Debug)]
pub enum InitramfsError {
    /// CPIO parsing error
    Cpio(CpioError),
    /// Invalid path in archive
    InvalidPath,
}

impl From<CpioError> for InitramfsError {
    fn from(e: CpioError) -> Self {
        InitramfsError::Cpio(e)
    }
}

/// Load an initramfs from CPIO data
///
/// Returns the root directory of the initramfs filesystem.
pub fn load(data: &[u8]) -> Result<Arc<InitramfsDir>, InitramfsError> {
    let root = Arc::new(InitramfsDir::new(Mode::DEFAULT_DIR));

    for entry_result in cpio::CpioIterator::new(data) {
        let entry = entry_result?;

        // Skip "." entry
        if entry.name == "." || entry.name.is_empty() {
            continue;
        }

        // Remove leading "./" if present
        let path = entry.name.strip_prefix("./").unwrap_or(&entry.name);
        if path.is_empty() {
            continue;
        }

        // Split path into components
        let components: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if components.is_empty() {
            continue;
        }

        // Navigate to parent directory, creating intermediate dirs as needed
        let mut current_dir = root.clone();
        for &component in &components[..components.len() - 1] {
            current_dir = current_dir.get_or_create_subdir(component);
        }

        // Add the final entry
        let name = components[components.len() - 1];

        if entry.is_dir() {
            // Create directory
            let _ = current_dir.get_or_create_subdir(name);
        } else if entry.is_file() {
            // Create file
            let perms = entry.permissions();
            let file = Arc::new(InitramfsFile {
                data: entry.data,
                ino: alloc_inode(),
                mode: Mode::new(perms),
            });
            current_dir.add_entry(name, InitramfsEntry::File(file));
        } else if entry.is_symlink() {
            // Create symlink
            let target = entry.symlink_target().unwrap_or("").to_string();
            let symlink = Arc::new(InitramfsSymlink {
                target,
                ino: alloc_inode(),
                mode: Mode::new(entry.permissions()),
            });
            current_dir.add_entry(name, InitramfsEntry::Symlink(symlink));
        } else if entry.is_char_device() || entry.is_block_device() {
            // Create device node
            let vtype = if entry.is_char_device() {
                VnodeType::CharDevice
            } else {
                VnodeType::BlockDevice
            };
            let (major, minor) = entry.device_numbers();
            let device = Arc::new(InitramfsDevice {
                vtype,
                major,
                minor,
                ino: alloc_inode(),
                mode: Mode::new(entry.permissions()),
            });
            current_dir.add_entry(name, InitramfsEntry::Device(device));
        }
        // FIFOs and sockets are skipped (not typically in initramfs)
    }

    Ok(root)
}

/// Create an empty initramfs
pub fn empty() -> Arc<InitramfsDir> {
    Arc::new(InitramfsDir::new(Mode::DEFAULT_DIR))
}
