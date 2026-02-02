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
    /// Mount flags (Linux-compatible values)
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct MountFlags: u32 {
        /// Read-only mount
        const MS_RDONLY = 1;
        /// Don't allow setuid/setgid
        const MS_NOSUID = 2;
        /// Don't interpret special files
        const MS_NODEV = 4;
        /// Don't allow program execution
        const MS_NOEXEC = 8;
        /// Writes are synced immediately
        const MS_SYNCHRONOUS = 16;
        /// Remount an existing mount
        const MS_REMOUNT = 32;
        /// Allow mandatory locks on this filesystem
        const MS_MANDLOCK = 64;
        /// Directory modifications are synchronous
        const MS_DIRSYNC = 128;
        /// Don't follow symlinks
        const MS_NOSYMFOLLOW = 256;
        /// Don't update access times
        const MS_NOATIME = 1024;
        /// Don't update directory access times
        const MS_NODIRATIME = 2048;
        /// Bind mount
        const MS_BIND = 4096;
        /// Move mount
        const MS_MOVE = 8192;
        /// Recursive mount
        const MS_REC = 16384;
        /// Silent flag
        const MS_SILENT = 32768;
        /// Relative atime (update atime relative to mtime/ctime)
        const MS_RELATIME = 1 << 21;
        /// Strict atime updates
        const MS_STRICTATIME = 1 << 24;
        /// Make writes sync lazily
        const MS_LAZYTIME = 1 << 25;
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

    /// Move a mount from one path to another
    ///
    /// Removes the mount at `from` and re-inserts it at `to`.
    pub fn move_mount(&self, from: &str, to: &str) -> VfsResult<()> {
        let mut mounts = self.mounts.write();
        let old_mount = mounts.remove(from).ok_or(VfsError::NotFound)?;
        let new_mount = Arc::new(Mount::new(
            old_mount.root().clone(),
            String::from(to),
            old_mount.flags(),
            old_mount.fs_type().to_string(),
        ));
        mounts.insert(String::from(to), new_mount);
        Ok(())
    }

    /// Pivot the root filesystem
    ///
    /// Makes the filesystem mounted at `new_root` become the new `/`,
    /// and moves the old root to `put_old`. All existing mount paths
    /// are rewritten accordingly.
    ///
    /// `put_old` must be under `new_root` (e.g., `/mnt/root/initramfs`).
    pub fn pivot_root(&self, new_root: &str, put_old: &str) -> VfsResult<()> {
        // Validate that put_old is under new_root
        if !put_old.starts_with(new_root) {
            return Err(VfsError::InvalidArgument);
        }

        // The suffix that put_old has after new_root (e.g., "/initramfs")
        let put_old_suffix = &put_old[new_root.len()..];

        let mut mounts = self.mounts.write();

        // Remove the new_root mount — it becomes "/"
        let new_root_mount = mounts.remove(new_root).ok_or(VfsError::NotFound)?;

        // Collect all current mount paths and their Arc<Mount>
        let entries: Vec<(String, Arc<Mount>)> =
            mounts.iter().map(|(k, v)| (k.clone(), v.clone())).collect();

        // Clear and rebuild with rewritten paths
        mounts.clear();

        let new_root_prefix = if new_root.ends_with('/') {
            new_root.to_string()
        } else {
            alloc::format!("{}/", new_root)
        };

        for (path, mount) in entries {
            let new_path = if path.starts_with(&new_root_prefix) {
                // Under new_root: strip prefix → becomes top-level
                // e.g., "/mnt/root/dev" → "/dev"
                let suffix = &path[new_root.len()..];
                String::from(suffix)
            } else {
                // Under old root: prepend put_old_suffix
                // e.g., "/" → "/initramfs", "/dev" → "/initramfs/dev"
                if path == "/" {
                    String::from(put_old_suffix)
                } else {
                    alloc::format!("{}{}", put_old_suffix, path)
                }
            };

            let remounted = Arc::new(Mount::new(
                mount.root().clone(),
                new_path.clone(),
                mount.flags(),
                mount.fs_type().to_string(),
            ));
            mounts.insert(new_path, remounted);
        }

        // Insert the new root at "/"
        let root_mount = Arc::new(Mount::new(
            new_root_mount.root().clone(),
            String::from("/"),
            new_root_mount.flags(),
            new_root_mount.fs_type().to_string(),
        ));
        mounts.insert(String::from("/"), root_mount.clone());

        // Drop mounts lock before acquiring root lock to avoid nested locking
        drop(mounts);

        // Update self.root
        let mut root_lock = self.root.write();
        *root_lock = Some(root_mount);

        Ok(())
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
            "initramfs" => 0x01021994, // INITRAMFS_MAGIC
            "oxidefs" => 0x4F584944,   // "OXID"
            "tmpfs" => 0x01021994,     // TMPFS_MAGIC
            "devfs" => 0x1373,         // DEVFS_MAGIC
            "procfs" => 0x9FA0,        // PROC_SUPER_MAGIC
            "sysfs" => 0x62656572,     // SYSFS_MAGIC
            _ => 0x4F584944,           // Default: OXID
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
