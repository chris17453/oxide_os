//! Vnode abstraction
//!
//! A vnode represents a file, directory, or other filesystem object.

use alloc::string::String;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicU64, Ordering};

use crate::error::VfsResult;

/// File type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VnodeType {
    /// Regular file
    File,
    /// Directory
    Directory,
    /// Symbolic link
    Symlink,
    /// Character device
    CharDevice,
    /// Block device
    BlockDevice,
    /// FIFO (named pipe)
    Fifo,
    /// Socket
    Socket,
}

/// File mode (permissions)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mode(pub u32);

impl Mode {
    /// Owner read permission
    pub const S_IRUSR: u32 = 0o400;
    /// Owner write permission
    pub const S_IWUSR: u32 = 0o200;
    /// Owner execute permission
    pub const S_IXUSR: u32 = 0o100;
    /// Group read permission
    pub const S_IRGRP: u32 = 0o040;
    /// Group write permission
    pub const S_IWGRP: u32 = 0o020;
    /// Group execute permission
    pub const S_IXGRP: u32 = 0o010;
    /// Other read permission
    pub const S_IROTH: u32 = 0o004;
    /// Other write permission
    pub const S_IWOTH: u32 = 0o002;
    /// Other execute permission
    pub const S_IXOTH: u32 = 0o001;

    /// Default file permissions (0644)
    pub const DEFAULT_FILE: Mode = Mode(0o644);
    /// Default directory permissions (0755)
    pub const DEFAULT_DIR: Mode = Mode(0o755);

    pub fn new(mode: u32) -> Self {
        Mode(mode & 0o7777)
    }

    pub fn bits(&self) -> u32 {
        self.0
    }
}

/// File statistics
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Stat {
    /// Device ID
    pub dev: u64,
    /// Inode number
    pub ino: u64,
    /// File mode (type and permissions)
    pub mode: u32,
    /// Number of hard links
    pub nlink: u64,
    /// Owner user ID
    pub uid: u32,
    /// Owner group ID
    pub gid: u32,
    /// Device ID (for special files)
    pub rdev: u64,
    /// File size in bytes
    pub size: u64,
    /// Block size for I/O
    pub blksize: u64,
    /// Number of 512-byte blocks allocated
    pub blocks: u64,
    /// Access time (seconds since epoch)
    pub atime: u64,
    /// Modification time (seconds since epoch)
    pub mtime: u64,
    /// Status change time (seconds since epoch)
    pub ctime: u64,
}

impl Stat {
    pub fn new(vtype: VnodeType, mode: Mode, size: u64, ino: u64) -> Self {
        let type_bits = match vtype {
            VnodeType::File => 0o100000,        // S_IFREG
            VnodeType::Directory => 0o040000,   // S_IFDIR
            VnodeType::Symlink => 0o120000,     // S_IFLNK
            VnodeType::CharDevice => 0o020000,  // S_IFCHR
            VnodeType::BlockDevice => 0o060000, // S_IFBLK
            VnodeType::Fifo => 0o010000,        // S_IFIFO
            VnodeType::Socket => 0o140000,      // S_IFSOCK
        };

        Stat {
            dev: 0,
            ino,
            mode: type_bits | mode.bits(),
            nlink: 1,
            uid: 0,
            gid: 0,
            rdev: 0,
            size,
            blksize: 4096,
            blocks: (size + 511) / 512,
            atime: 0,
            mtime: 0,
            ctime: 0,
        }
    }
}

/// Directory entry
#[derive(Debug, Clone)]
pub struct DirEntry {
    /// Entry name
    pub name: String,
    /// Inode number
    pub ino: u64,
    /// File type
    pub file_type: VnodeType,
}

/// Vnode operations trait
///
/// Each filesystem implements this trait to provide file operations.
pub trait VnodeOps: Send + Sync {
    /// Get vnode type
    fn vtype(&self) -> VnodeType;

    /// Look up a name in this directory
    fn lookup(&self, name: &str) -> VfsResult<Arc<dyn VnodeOps>>;

    /// Create a file in this directory
    fn create(&self, name: &str, mode: Mode) -> VfsResult<Arc<dyn VnodeOps>>;

    /// Read data from this file
    fn read(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize>;

    /// Write data to this file
    fn write(&self, offset: u64, buf: &[u8]) -> VfsResult<usize>;

    /// Read directory entries
    fn readdir(&self, offset: u64) -> VfsResult<Option<DirEntry>>;

    /// Create a directory
    fn mkdir(&self, name: &str, mode: Mode) -> VfsResult<Arc<dyn VnodeOps>>;

    /// Remove a directory
    fn rmdir(&self, name: &str) -> VfsResult<()>;

    /// Remove a file
    fn unlink(&self, name: &str) -> VfsResult<()>;

    /// Rename a file
    fn rename(&self, old_name: &str, new_dir: &dyn VnodeOps, new_name: &str) -> VfsResult<()>;

    /// Get file statistics
    fn stat(&self) -> VfsResult<Stat>;

    /// Truncate file to given size
    fn truncate(&self, size: u64) -> VfsResult<()>;

    /// Get file size
    fn size(&self) -> u64 {
        self.stat().map(|s| s.size).unwrap_or(0)
    }

    /// Perform device-specific I/O control operation
    ///
    /// Default implementation returns NotSupported for non-device files.
    fn ioctl(&self, _request: u64, _arg: u64) -> VfsResult<i64> {
        Err(crate::error::VfsError::NotSupported)
    }

    /// Change file mode bits
    ///
    /// Default implementation returns NotSupported.
    fn chmod(&self, _mode: u32) -> VfsResult<()> {
        Err(crate::error::VfsError::NotSupported)
    }

    /// Change file owner and group
    ///
    /// Default implementation returns NotSupported.
    fn chown(&self, _uid: Option<u32>, _gid: Option<u32>) -> VfsResult<()> {
        Err(crate::error::VfsError::NotSupported)
    }

    /// Create a hard link
    ///
    /// Default implementation returns NotSupported.
    fn link(&self, _name: &str, _target: &dyn VnodeOps) -> VfsResult<()> {
        Err(crate::error::VfsError::NotSupported)
    }

    /// Create a symbolic link
    ///
    /// Default implementation returns NotSupported.
    fn symlink(&self, _name: &str, _target: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(crate::error::VfsError::NotSupported)
    }

    /// Read symbolic link target
    ///
    /// Default implementation returns NotSupported.
    fn readlink(&self) -> VfsResult<String> {
        Err(crate::error::VfsError::NotSupported)
    }

    /// Set file access and modification times
    ///
    /// # Arguments
    /// * `atime` - Optional new access time (seconds since epoch), None = don't change
    /// * `mtime` - Optional new modification time (seconds since epoch), None = don't change
    ///
    /// Default implementation returns NotSupported.
    fn set_times(&self, _atime: Option<u64>, _mtime: Option<u64>) -> VfsResult<()> {
        Err(crate::error::VfsError::NotSupported)
    }
}

/// Vnode wrapper that adds reference counting and caching
pub struct Vnode {
    /// The underlying vnode operations
    ops: Arc<dyn VnodeOps>,
    /// Vnode ID (unique within mount)
    id: u64,
}

impl Vnode {
    /// Create a new vnode
    pub fn new(ops: Arc<dyn VnodeOps>, id: u64) -> Self {
        Vnode { ops, id }
    }

    /// Get the vnode ID
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Get the vnode operations
    pub fn ops(&self) -> &Arc<dyn VnodeOps> {
        &self.ops
    }

    /// Get vnode type
    pub fn vtype(&self) -> VnodeType {
        self.ops.vtype()
    }

    /// Is this a directory?
    pub fn is_dir(&self) -> bool {
        self.vtype() == VnodeType::Directory
    }

    /// Is this a regular file?
    pub fn is_file(&self) -> bool {
        self.vtype() == VnodeType::File
    }

    /// Change file mode bits
    pub fn chmod(&self, mode: u32) -> VfsResult<()> {
        self.ops.chmod(mode)
    }

    /// Change file owner and group
    pub fn chown(&self, uid: Option<u32>, gid: Option<u32>) -> VfsResult<()> {
        self.ops.chown(uid, gid)
    }
}

/// Inode number allocator
pub struct InodeAllocator {
    next: AtomicU64,
}

impl InodeAllocator {
    pub const fn new() -> Self {
        InodeAllocator {
            next: AtomicU64::new(1),
        }
    }

    pub fn alloc(&self) -> u64 {
        self.next.fetch_add(1, Ordering::Relaxed)
    }
}

impl Default for InodeAllocator {
    fn default() -> Self {
        Self::new()
    }
}
