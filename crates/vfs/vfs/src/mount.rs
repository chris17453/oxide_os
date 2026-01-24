//! Mount points and VFS management
//!
//! Handles mounting filesystems and path resolution across mount boundaries.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::RwLock;

use crate::error::{VfsError, VfsResult};
use crate::path::Path;
use crate::vnode::VnodeOps;

use bitflags::bitflags;

bitflags! {
    /// Mount flags
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct MountFlags: u32 {
        /// Read-only mount
        const MS_RDONLY = 1;
        /// Don't update access times
        const MS_NOATIME = 2;
        /// Don't allow setuid/setgid
        const MS_NOSUID = 4;
        /// Don't interpret special files
        const MS_NODEV = 8;
        /// Don't allow program execution
        const MS_NOEXEC = 16;
    }
}

/// A mounted filesystem
pub struct Mount {
    /// Filesystem root vnode
    root: Arc<dyn VnodeOps>,
    /// Mount point path
    mount_point: String,
    /// Mount flags
    flags: MountFlags,
    /// Filesystem type name
    fs_type: String,
}

impl Mount {
    /// Create a new mount
    pub fn new(
        root: Arc<dyn VnodeOps>,
        mount_point: String,
        flags: MountFlags,
        fs_type: String,
    ) -> Self {
        Mount {
            root,
            mount_point,
            flags,
            fs_type,
        }
    }

    /// Get the root vnode
    pub fn root(&self) -> &Arc<dyn VnodeOps> {
        &self.root
    }

    /// Get mount point path
    pub fn mount_point(&self) -> &str {
        &self.mount_point
    }

    /// Get mount flags
    pub fn flags(&self) -> MountFlags {
        self.flags
    }

    /// Get filesystem type
    pub fn fs_type(&self) -> &str {
        &self.fs_type
    }

    /// Is this mount read-only?
    pub fn is_readonly(&self) -> bool {
        self.flags.contains(MountFlags::MS_RDONLY)
    }
}

/// The virtual filesystem
pub struct VFS {
    /// Mounted filesystems by mount point
    mounts: RwLock<BTreeMap<String, Arc<Mount>>>,
    /// Root filesystem
    root: RwLock<Option<Arc<Mount>>>,
}

impl VFS {
    /// Create a new VFS
    pub const fn new() -> Self {
        VFS {
            mounts: RwLock::new(BTreeMap::new()),
            root: RwLock::new(None),
        }
    }

    /// Mount a filesystem
    pub fn mount(
        &self,
        root: Arc<dyn VnodeOps>,
        mount_point: &str,
        flags: MountFlags,
        fs_type: &str,
    ) -> VfsResult<()> {
        let mount = Arc::new(Mount::new(
            root,
            String::from(mount_point),
            flags,
            String::from(fs_type),
        ));

        if mount_point == "/" {
            let mut root_lock = self.root.write();
            *root_lock = Some(mount.clone());
        }

        let mut mounts = self.mounts.write();
        mounts.insert(String::from(mount_point), mount);
        Ok(())
    }

    /// Unmount a filesystem
    pub fn unmount(&self, mount_point: &str) -> VfsResult<()> {
        if mount_point == "/" {
            return Err(VfsError::Busy);
        }

        let mut mounts = self.mounts.write();
        mounts.remove(mount_point).ok_or(VfsError::NotFound)?;
        Ok(())
    }

    /// Find the mount for a given path
    fn find_mount(&self, path: &str) -> Option<(Arc<Mount>, String)> {
        let mounts = self.mounts.read();

        // Find longest matching mount point
        let mut best_match: Option<(&String, &Arc<Mount>)> = None;

        for (mount_point, mount) in mounts.iter() {
            if path.starts_with(mount_point.as_str())
                || (mount_point == "/" && path.starts_with('/'))
            {
                match best_match {
                    None => best_match = Some((mount_point, mount)),
                    Some((best_mp, _)) if mount_point.len() > best_mp.len() => {
                        best_match = Some((mount_point, mount));
                    }
                    _ => {}
                }
            }
        }

        best_match.map(|(mp, mount)| {
            let remaining = if mp == "/" {
                path.to_string()
            } else {
                path.strip_prefix(mp.as_str()).unwrap_or(path).to_string()
            };
            (mount.clone(), remaining)
        })
    }

    /// Resolve a path to a vnode
    pub fn lookup(&self, path: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        let normalized = Path::new(path).normalize();
        let path_str = normalized.to_string();

        let (mount, remaining) = self.find_mount(&path_str).ok_or(VfsError::NotFound)?;

        let mut current = mount.root().clone();

        // Walk the path components
        let remaining_path = Path::new(&remaining);
        for component in remaining_path.components() {
            if component.is_empty() {
                continue;
            }
            current = current.lookup(component)?;
        }

        Ok(current)
    }

    /// Resolve a path to parent directory and filename
    pub fn lookup_parent(&self, path: &str) -> VfsResult<(Arc<dyn VnodeOps>, String)> {
        let normalized = Path::new(path).normalize();

        let filename = normalized
            .filename()
            .ok_or(VfsError::InvalidArgument)?
            .to_string();

        let parent_path = normalized
            .parent()
            .map(|p| p.to_string())
            .unwrap_or_else(|| String::from("/"));

        let parent = self.lookup(&parent_path)?;
        Ok((parent, filename))
    }

    /// Get the root vnode
    pub fn root(&self) -> VfsResult<Arc<dyn VnodeOps>> {
        self.root
            .read()
            .as_ref()
            .map(|m| m.root().clone())
            .ok_or(VfsError::NotFound)
    }

    /// List all mount points
    pub fn mounts(&self) -> Vec<String> {
        self.mounts.read().keys().cloned().collect()
    }

    /// Get filesystem statistics for a path
    pub fn statfs(&self, path: &str) -> VfsResult<FsInfo> {
        // Find the mount for this path
        let (mount, _relative_path) = self.find_mount(path).ok_or(VfsError::NotFound)?;

        // Get basic stats from the mount
        let info = FsInfo {
            fs_type: self.fs_type_magic(mount.fs_type()),
            block_size: 4096,
            total_blocks: 0,
            free_blocks: 0,
            available_blocks: 0,
            total_inodes: 0,
            free_inodes: 0,
            max_name_len: 255,
        };

        Ok(info)
    }

    /// Convert filesystem type name to magic number
    fn fs_type_magic(&self, fs_type: &str) -> u32 {
        match fs_type {
            "initramfs" => 0x01021994,  // INITRAMFS_MAGIC
            "oxidefs" => 0x4F584944,    // "OXID"
            "tmpfs" => 0x01021994,      // TMPFS_MAGIC
            "devfs" => 0x1373,          // DEVFS_MAGIC
            "procfs" => 0x9FA0,         // PROC_SUPER_MAGIC
            "sysfs" => 0x62656572,      // SYSFS_MAGIC
            _ => 0x4F584944,            // Default: OXID
        }
    }
}

/// Filesystem information (for statfs)
#[derive(Debug, Clone, Copy)]
pub struct FsInfo {
    /// Filesystem type (magic number)
    pub fs_type: u32,
    /// Block size
    pub block_size: u32,
    /// Total data blocks
    pub total_blocks: u64,
    /// Free blocks
    pub free_blocks: u64,
    /// Available blocks (to unprivileged user)
    pub available_blocks: u64,
    /// Total inodes
    pub total_inodes: u64,
    /// Free inodes
    pub free_inodes: u64,
    /// Maximum filename length
    pub max_name_len: u32,
}

impl Default for VFS {
    fn default() -> Self {
        Self::new()
    }
}

/// Global VFS instance
pub static GLOBAL_VFS: VFS = VFS::new();

/// Get filesystem statistics for a path (global helper)
pub fn vfs_statfs(path: &str) -> VfsResult<FsInfo> {
    GLOBAL_VFS.statfs(path)
}
