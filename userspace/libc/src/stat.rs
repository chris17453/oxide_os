//! File status operations
//!
//! Provides stat, fstat, and related functions.

use crate::syscall;

/// File type and mode
pub const S_IFMT: u32 = 0o170000; // Type of file mask
pub const S_IFSOCK: u32 = 0o140000; // Socket
pub const S_IFLNK: u32 = 0o120000; // Symbolic link
pub const S_IFREG: u32 = 0o100000; // Regular file
pub const S_IFBLK: u32 = 0o060000; // Block device
pub const S_IFDIR: u32 = 0o040000; // Directory
pub const S_IFCHR: u32 = 0o020000; // Character device
pub const S_IFIFO: u32 = 0o010000; // FIFO

/// Stat structure matching kernel's format exactly
///
/// This must be repr(C) to match the kernel's ABI.
/// The C ABI will add implicit padding between mode (u32) and nlink (u64).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Stat {
    /// Device ID
    pub dev: u64,
    /// Inode number
    pub ino: u64,
    /// File mode (type and permissions)
    pub mode: u32,
    /// Number of hard links
    pub nlink: u64,
    /// User ID
    pub uid: u32,
    /// Group ID
    pub gid: u32,
    /// Device ID (if special file)
    pub rdev: u64,
    /// Total size in bytes
    pub size: u64,
    /// Block size for I/O
    pub blksize: u64,
    /// Number of 512-byte blocks
    pub blocks: u64,
    /// Access time (seconds since epoch)
    pub atime: u64,
    /// Modification time (seconds since epoch)
    pub mtime: u64,
    /// Status change time (seconds since epoch)
    pub ctime: u64,
}

impl Stat {
    /// Create a zeroed stat struct
    pub const fn zeroed() -> Self {
        Stat {
            dev: 0,
            ino: 0,
            mode: 0,
            nlink: 0,
            uid: 0,
            gid: 0,
            rdev: 0,
            size: 0,
            blksize: 0,
            blocks: 0,
            atime: 0,
            mtime: 0,
            ctime: 0,
        }
    }

    /// Check if this is a regular file
    pub fn is_file(&self) -> bool {
        (self.mode & S_IFMT) == S_IFREG
    }

    /// Check if this is a directory
    pub fn is_dir(&self) -> bool {
        (self.mode & S_IFMT) == S_IFDIR
    }

    /// Check if this is a symbolic link
    pub fn is_symlink(&self) -> bool {
        (self.mode & S_IFMT) == S_IFLNK
    }

    /// Check if this is a character device
    pub fn is_char_device(&self) -> bool {
        (self.mode & S_IFMT) == S_IFCHR
    }

    /// Check if this is a block device
    pub fn is_block_device(&self) -> bool {
        (self.mode & S_IFMT) == S_IFBLK
    }

    /// Check if this is a FIFO (named pipe)
    pub fn is_fifo(&self) -> bool {
        (self.mode & S_IFMT) == S_IFIFO
    }

    /// Check if this is a socket
    pub fn is_socket(&self) -> bool {
        (self.mode & S_IFMT) == S_IFSOCK
    }
}

/// Get file status by path
pub fn stat(path: &str, statbuf: &mut Stat) -> i32 {
    syscall::syscall3(
        syscall::nr::STAT,
        path.as_ptr() as usize,
        path.len(),
        statbuf as *mut Stat as usize,
    ) as i32
}

/// Get file status by file descriptor
pub fn fstat(fd: i32, statbuf: &mut Stat) -> i32 {
    syscall::syscall2(
        syscall::nr::FSTAT,
        fd as usize,
        statbuf as *mut Stat as usize,
    ) as i32
}

/// Get file status by path, not following symlinks
///
/// Returns information about the symbolic link itself rather than its target.
pub fn lstat(path: &str, statbuf: &mut Stat) -> i32 {
    syscall::syscall3(
        syscall::nr::LSTAT,
        path.as_ptr() as usize,
        path.len(),
        statbuf as *mut Stat as usize,
    ) as i32
}
