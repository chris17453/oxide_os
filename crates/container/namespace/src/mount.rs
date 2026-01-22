//! Mount Namespace

use crate::{NsError, NsResult, alloc_ns_id};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::RwLock;

/// Mount point
#[derive(Clone)]
pub struct Mount {
    /// Mount ID
    pub id: u64,
    /// Parent mount ID
    pub parent_id: u64,
    /// Device ID
    pub dev_id: u64,
    /// Root of mount
    pub root: String,
    /// Mount point
    pub mountpoint: String,
    /// Filesystem type
    pub fstype: String,
    /// Mount options
    pub options: String,
    /// Mount flags
    pub flags: MountFlags,
}

/// Mount flags
#[derive(Clone, Copy, Default)]
pub struct MountFlags {
    /// Read-only
    pub readonly: bool,
    /// No setuid
    pub nosuid: bool,
    /// No device access
    pub nodev: bool,
    /// No execute
    pub noexec: bool,
    /// Synchronous
    pub synchronous: bool,
    /// No atime updates
    pub noatime: bool,
    /// Bind mount
    pub bind: bool,
    /// Move mount
    pub move_: bool,
    /// Recursive
    pub rec: bool,
    /// Shared mount
    pub shared: bool,
    /// Slave mount
    pub slave: bool,
    /// Private mount
    pub private: bool,
    /// Unbindable mount
    pub unbindable: bool,
}

/// Mount propagation type
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PropagationType {
    /// Private mount
    Private,
    /// Shared mount
    Shared,
    /// Slave mount
    Slave,
    /// Unbindable mount
    Unbindable,
}

/// Mount namespace
pub struct MountNamespace {
    /// Unique namespace ID
    id: u64,
    /// Parent namespace
    parent: Option<Arc<MountNamespace>>,
    /// Root mount
    root: RwLock<Option<Mount>>,
    /// All mounts
    mounts: RwLock<Vec<Mount>>,
    /// Next mount ID
    next_mount_id: AtomicU64,
}

impl MountNamespace {
    /// Create root mount namespace
    pub fn root() -> Self {
        MountNamespace {
            id: alloc_ns_id(),
            parent: None,
            root: RwLock::new(Some(Mount {
                id: 1,
                parent_id: 0,
                dev_id: 0,
                root: String::from("/"),
                mountpoint: String::from("/"),
                fstype: String::from("rootfs"),
                options: String::from("rw"),
                flags: MountFlags::default(),
            })),
            mounts: RwLock::new(Vec::new()),
            next_mount_id: AtomicU64::new(2),
        }
    }

    /// Create child mount namespace (copy-on-write)
    pub fn new(parent: Option<Arc<MountNamespace>>) -> Self {
        let (root, mounts, next_id) = if let Some(ref p) = parent {
            (
                p.root.read().clone(),
                p.mounts.read().clone(),
                p.next_mount_id.load(Ordering::SeqCst),
            )
        } else {
            (None, Vec::new(), 2)
        };

        MountNamespace {
            id: alloc_ns_id(),
            parent,
            root: RwLock::new(root),
            mounts: RwLock::new(mounts),
            next_mount_id: AtomicU64::new(next_id),
        }
    }

    /// Get namespace ID
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Allocate mount ID
    fn alloc_mount_id(&self) -> u64 {
        self.next_mount_id.fetch_add(1, Ordering::SeqCst)
    }

    /// Mount a filesystem
    pub fn mount(
        &self,
        source: &str,
        target: &str,
        fstype: &str,
        flags: MountFlags,
        options: &str,
    ) -> NsResult<u64> {
        let id = self.alloc_mount_id();

        // Find parent mount
        let parent_id = self.find_mount_at(target).map(|m| m.id).unwrap_or(1);

        let mount = Mount {
            id,
            parent_id,
            dev_id: 0, // Would be assigned by VFS
            root: String::from(source),
            mountpoint: String::from(target),
            fstype: String::from(fstype),
            options: String::from(options),
            flags,
        };

        self.mounts.write().push(mount);
        Ok(id)
    }

    /// Unmount a filesystem
    pub fn umount(&self, target: &str, _flags: u32) -> NsResult<()> {
        let mut mounts = self.mounts.write();

        if let Some(pos) = mounts.iter().position(|m| m.mountpoint == target) {
            // Check if anything is mounted under this
            let mount_id = mounts[pos].id;
            if mounts.iter().any(|m| m.parent_id == mount_id) {
                return Err(NsError::Busy);
            }
            mounts.remove(pos);
            Ok(())
        } else {
            Err(NsError::NotFound)
        }
    }

    /// Find mount at path
    pub fn find_mount_at(&self, path: &str) -> Option<Mount> {
        let mounts = self.mounts.read();

        // Find longest matching mountpoint
        mounts
            .iter()
            .filter(|m| path.starts_with(&m.mountpoint))
            .max_by_key(|m| m.mountpoint.len())
            .cloned()
    }

    /// Get all mounts
    pub fn mounts(&self) -> Vec<Mount> {
        self.mounts.read().clone()
    }

    /// Pivot root
    pub fn pivot_root(&self, new_root: &str, put_old: &str) -> NsResult<()> {
        // Verify new_root is a mount point
        let new_mount = self.find_mount_at(new_root).ok_or(NsError::NotFound)?;

        if new_mount.mountpoint != new_root {
            return Err(NsError::InvalidOperation);
        }

        // Update root
        *self.root.write() = Some(new_mount);

        Ok(())
    }

    /// Get root path
    pub fn root_path(&self) -> String {
        self.root
            .read()
            .as_ref()
            .map(|m| m.mountpoint.clone())
            .unwrap_or_else(|| String::from("/"))
    }
}
