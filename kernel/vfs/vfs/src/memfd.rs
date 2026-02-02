//! Memory-backed anonymous file (memfd_create)
//!
//! Creates an anonymous file living in memory, not linked to any directory.
//! Supports read, write, truncate, seek, and mmap-like operations.

use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::RwLock;

use crate::error::{VfsError, VfsResult};
use crate::vnode::{DirEntry, Mode, Stat, VnodeOps, VnodeType};

/// Memory-backed anonymous file
pub struct MemFd {
    /// File contents
    data: RwLock<Vec<u8>>,
    /// Seal flags (MFD_ALLOW_SEALING support)
    seals: RwLock<u32>,
}

/// memfd_create flags
pub const MFD_CLOEXEC: u32 = 0x0001;
pub const MFD_ALLOW_SEALING: u32 = 0x0002;

/// Seal flags for fcntl F_ADD_SEALS
pub const F_SEAL_SEAL: u32 = 0x0001;
pub const F_SEAL_SHRINK: u32 = 0x0002;
pub const F_SEAL_GROW: u32 = 0x0004;
pub const F_SEAL_WRITE: u32 = 0x0008;

impl MemFd {
    /// Create a new memory-backed file
    pub fn new() -> Self {
        MemFd {
            data: RwLock::new(Vec::new()),
            seals: RwLock::new(0),
        }
    }
}

impl VnodeOps for MemFd {
    fn vtype(&self) -> VnodeType {
        VnodeType::File
    }

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotSupported)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotSupported)
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
        let seals = *self.seals.read();
        if seals & F_SEAL_WRITE != 0 {
            return Err(VfsError::PermissionDenied);
        }

        let mut data = self.data.write();
        let offset = offset as usize;

        let required_len = offset + buf.len();
        if required_len > data.len() {
            if seals & F_SEAL_GROW != 0 {
                return Err(VfsError::PermissionDenied);
            }
            data.resize(required_len, 0);
        }

        data[offset..offset + buf.len()].copy_from_slice(buf);
        Ok(buf.len())
    }

    fn readdir(&self, _offset: u64) -> VfsResult<Option<DirEntry>> {
        Err(VfsError::NotSupported)
    }

    fn mkdir(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotSupported)
    }

    fn rmdir(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }

    fn unlink(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }

    fn rename(&self, _old: &str, _new_dir: &dyn VnodeOps, _new: &str) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }

    fn stat(&self) -> VfsResult<Stat> {
        let size = self.data.read().len() as u64;
        Ok(Stat::new(VnodeType::File, Mode::new(0o600), size, 0))
    }

    fn truncate(&self, size: u64) -> VfsResult<()> {
        let seals = *self.seals.read();
        if seals & F_SEAL_SHRINK != 0 && (size as usize) < self.data.read().len() {
            return Err(VfsError::PermissionDenied);
        }
        if seals & F_SEAL_GROW != 0 && (size as usize) > self.data.read().len() {
            return Err(VfsError::PermissionDenied);
        }
        self.data.write().resize(size as usize, 0);
        Ok(())
    }

    fn size(&self) -> u64 {
        self.data.read().len() as u64
    }

    fn poll_read_ready(&self) -> bool {
        true
    }

    fn poll_write_ready(&self) -> bool {
        *self.seals.read() & F_SEAL_WRITE == 0
    }
}

/// Create a new memory-backed anonymous file vnode
pub fn create_memfd() -> Arc<dyn VnodeOps> {
    Arc::new(MemFd::new())
}
