//! In-memory temporary filesystem (tmpfs) for EFFLUX OS
//!
//! Provides a fully functional in-memory filesystem for storing files and directories.

#![no_std]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::RwLock;

use efflux_vfs::{
    DirEntry, Mode, Stat, VfsError, VfsResult, VnodeOps, VnodeType,
};

/// Inode allocator for tmpfs
static TMPFS_INODE: AtomicU64 = AtomicU64::new(1);

fn alloc_inode() -> u64 {
    TMPFS_INODE.fetch_add(1, Ordering::Relaxed)
}

/// A directory entry in tmpfs
enum TmpEntry {
    File(Arc<TmpFile>),
    Dir(Arc<TmpDir>),
}

impl TmpEntry {
    fn as_vnode(&self) -> Arc<dyn VnodeOps> {
        match self {
            TmpEntry::File(f) => f.clone(),
            TmpEntry::Dir(d) => d.clone(),
        }
    }

    fn vtype(&self) -> VnodeType {
        match self {
            TmpEntry::File(_) => VnodeType::File,
            TmpEntry::Dir(_) => VnodeType::Directory,
        }
    }

    fn ino(&self) -> u64 {
        match self {
            TmpEntry::File(f) => f.ino,
            TmpEntry::Dir(d) => d.ino,
        }
    }
}

/// A tmpfs directory
pub struct TmpDir {
    /// Directory entries
    entries: RwLock<BTreeMap<String, TmpEntry>>,
    /// Inode number
    ino: u64,
    /// Mode/permissions
    mode: Mode,
}

impl TmpDir {
    /// Create a new tmpfs root directory
    pub fn new_root() -> Arc<Self> {
        Arc::new(TmpDir {
            entries: RwLock::new(BTreeMap::new()),
            ino: alloc_inode(),
            mode: Mode::DEFAULT_DIR,
        })
    }

    /// Create a new subdirectory
    fn new_subdir(mode: Mode) -> Arc<Self> {
        Arc::new(TmpDir {
            entries: RwLock::new(BTreeMap::new()),
            ino: alloc_inode(),
            mode,
        })
    }
}

impl VnodeOps for TmpDir {
    fn vtype(&self) -> VnodeType {
        VnodeType::Directory
    }

    fn lookup(&self, name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        let entries = self.entries.read();
        entries
            .get(name)
            .map(|e| e.as_vnode())
            .ok_or(VfsError::NotFound)
    }

    fn create(&self, name: &str, mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        if !efflux_vfs::path::is_valid_name(name) {
            return Err(VfsError::InvalidArgument);
        }

        let mut entries = self.entries.write();

        if entries.contains_key(name) {
            return Err(VfsError::AlreadyExists);
        }

        let file = Arc::new(TmpFile::new(mode));
        entries.insert(name.to_string(), TmpEntry::File(file.clone()));
        Ok(file)
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
                ino: self.ino, // Would be parent ino, but we don't track parent
                file_type: VnodeType::Directory,
            }));
        }

        // Regular entries start at offset 2
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

    fn mkdir(&self, name: &str, mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        if !efflux_vfs::path::is_valid_name(name) {
            return Err(VfsError::InvalidArgument);
        }

        let mut entries = self.entries.write();

        if entries.contains_key(name) {
            return Err(VfsError::AlreadyExists);
        }

        let dir = TmpDir::new_subdir(mode);
        entries.insert(name.to_string(), TmpEntry::Dir(dir.clone()));
        Ok(dir)
    }

    fn rmdir(&self, name: &str) -> VfsResult<()> {
        let mut entries = self.entries.write();

        match entries.get(name) {
            None => return Err(VfsError::NotFound),
            Some(TmpEntry::File(_)) => return Err(VfsError::NotDirectory),
            Some(TmpEntry::Dir(dir)) => {
                // Check if directory is empty
                if !dir.entries.read().is_empty() {
                    return Err(VfsError::NotEmpty);
                }
            }
        }

        entries.remove(name);
        Ok(())
    }

    fn unlink(&self, name: &str) -> VfsResult<()> {
        let mut entries = self.entries.write();

        match entries.get(name) {
            None => return Err(VfsError::NotFound),
            Some(TmpEntry::Dir(_)) => return Err(VfsError::IsDirectory),
            Some(TmpEntry::File(_)) => {}
        }

        entries.remove(name);
        Ok(())
    }

    fn rename(&self, old_name: &str, new_dir: &dyn VnodeOps, new_name: &str) -> VfsResult<()> {
        // For simplicity, only support rename within same directory for now
        // Cross-directory rename would require downcasting new_dir
        let _ = new_dir;

        if !efflux_vfs::path::is_valid_name(new_name) {
            return Err(VfsError::InvalidArgument);
        }

        let mut entries = self.entries.write();

        let entry = entries.remove(old_name).ok_or(VfsError::NotFound)?;

        // Check if target exists and handle appropriately
        if let Some(existing) = entries.get(new_name) {
            match (&entry, existing) {
                (TmpEntry::File(_), TmpEntry::File(_)) => {
                    // Replacing file with file is OK
                }
                (TmpEntry::Dir(_), TmpEntry::Dir(d)) => {
                    // Replacing empty dir with dir is OK
                    if !d.entries.read().is_empty() {
                        entries.insert(old_name.to_string(), entry);
                        return Err(VfsError::NotEmpty);
                    }
                }
                _ => {
                    entries.insert(old_name.to_string(), entry);
                    return Err(VfsError::InvalidArgument);
                }
            }
        }

        entries.insert(new_name.to_string(), entry);
        Ok(())
    }

    fn stat(&self) -> VfsResult<Stat> {
        Ok(Stat::new(VnodeType::Directory, self.mode, 0, self.ino))
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::IsDirectory)
    }
}

/// A tmpfs file
pub struct TmpFile {
    /// File contents
    data: RwLock<Vec<u8>>,
    /// Inode number
    ino: u64,
    /// Mode/permissions
    mode: Mode,
}

impl TmpFile {
    /// Create a new empty file
    fn new(mode: Mode) -> Self {
        TmpFile {
            data: RwLock::new(Vec::new()),
            ino: alloc_inode(),
            mode,
        }
    }
}

impl VnodeOps for TmpFile {
    fn vtype(&self) -> VnodeType {
        VnodeType::File
    }

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn read(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        let data = self.data.read();
        let offset = offset as usize;

        if offset >= data.len() {
            return Ok(0);
        }

        let available = data.len() - offset;
        let to_read = buf.len().min(available);
        buf[..to_read].copy_from_slice(&data[offset..offset + to_read]);
        Ok(to_read)
    }

    fn write(&self, offset: u64, buf: &[u8]) -> VfsResult<usize> {
        let mut data = self.data.write();
        let offset = offset as usize;

        // Extend file if needed
        let required_len = offset + buf.len();
        if required_len > data.len() {
            data.resize(required_len, 0);
        }

        data[offset..offset + buf.len()].copy_from_slice(buf);
        Ok(buf.len())
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
        Err(VfsError::NotDirectory)
    }

    fn stat(&self) -> VfsResult<Stat> {
        let size = self.data.read().len() as u64;
        Ok(Stat::new(VnodeType::File, self.mode, size, self.ino))
    }

    fn truncate(&self, size: u64) -> VfsResult<()> {
        let mut data = self.data.write();
        data.resize(size as usize, 0);
        Ok(())
    }

    fn size(&self) -> u64 {
        self.data.read().len() as u64
    }
}

/// Create a new tmpfs instance
pub fn new_tmpfs() -> Arc<TmpDir> {
    TmpDir::new_root()
}
