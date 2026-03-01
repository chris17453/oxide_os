//! VFS error types

use core::fmt;

/// VFS error codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsError {
    /// No such file or directory
    NotFound,
    /// Permission denied
    PermissionDenied,
    /// File exists
    AlreadyExists,
    /// Not a directory
    NotDirectory,
    /// Is a directory
    IsDirectory,
    /// Invalid argument
    InvalidArgument,
    /// No space left on device
    NoSpace,
    /// Read-only filesystem
    ReadOnly,
    /// Too many open files
    TooManyOpenFiles,
    /// Bad file descriptor
    BadFd,
    /// Directory not empty
    NotEmpty,
    /// Cross-device link
    CrossDevice,
    /// Name too long
    NameTooLong,
    /// I/O error
    IoError,
    /// Not supported
    NotSupported,
    /// Busy
    Busy,
    /// No such device
    NoDevice,
    /// Broken pipe
    BrokenPipe,
    /// Operation would block
    WouldBlock,
    /// Interrupted by signal
    /// — GraveShift: The ghost of EINTR haunts every blocking syscall. You wanted Unix semantics?
    /// You got Unix semantics. Welcome to the pain.
    Interrupted,
    /// Invalid operation for file type
    InvalidOperation,
    /// Invalid/unrecognized filesystem
    InvalidFilesystem,
    /// Corrupted filesystem data
    CorruptedFilesystem,
}

impl VfsError {
    /// Convert to errno value
    pub fn to_errno(self) -> i32 {
        match self {
            VfsError::NotFound => -2,            // ENOENT
            VfsError::PermissionDenied => -13,   // EACCES
            VfsError::AlreadyExists => -17,      // EEXIST
            VfsError::NotDirectory => -20,       // ENOTDIR
            VfsError::IsDirectory => -21,        // EISDIR
            VfsError::InvalidArgument => -22,    // EINVAL
            VfsError::NoSpace => -28,            // ENOSPC
            VfsError::ReadOnly => -30,           // EROFS
            VfsError::TooManyOpenFiles => -24,   // EMFILE
            VfsError::BadFd => -9,               // EBADF
            VfsError::NotEmpty => -39,           // ENOTEMPTY
            VfsError::CrossDevice => -18,        // EXDEV
            VfsError::NameTooLong => -36,        // ENAMETOOLONG
            VfsError::IoError => -5,             // EIO
            VfsError::NotSupported => -95,       // ENOTSUP
            VfsError::Busy => -16,               // EBUSY
            VfsError::NoDevice => -19,           // ENODEV
            VfsError::BrokenPipe => -32,         // EPIPE
            VfsError::WouldBlock => -11,         // EAGAIN
            VfsError::Interrupted => -4,         // EINTR
            VfsError::InvalidOperation => -22,   // EINVAL
            VfsError::InvalidFilesystem => -22,  // EINVAL
            VfsError::CorruptedFilesystem => -5, // EIO
        }
    }
}

impl fmt::Display for VfsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VfsError::NotFound => write!(f, "No such file or directory"),
            VfsError::PermissionDenied => write!(f, "Permission denied"),
            VfsError::AlreadyExists => write!(f, "File exists"),
            VfsError::NotDirectory => write!(f, "Not a directory"),
            VfsError::IsDirectory => write!(f, "Is a directory"),
            VfsError::InvalidArgument => write!(f, "Invalid argument"),
            VfsError::NoSpace => write!(f, "No space left on device"),
            VfsError::ReadOnly => write!(f, "Read-only filesystem"),
            VfsError::TooManyOpenFiles => write!(f, "Too many open files"),
            VfsError::BadFd => write!(f, "Bad file descriptor"),
            VfsError::NotEmpty => write!(f, "Directory not empty"),
            VfsError::CrossDevice => write!(f, "Cross-device link"),
            VfsError::NameTooLong => write!(f, "Name too long"),
            VfsError::IoError => write!(f, "I/O error"),
            VfsError::NotSupported => write!(f, "Operation not supported"),
            VfsError::Busy => write!(f, "Device or resource busy"),
            VfsError::NoDevice => write!(f, "No such device"),
            VfsError::BrokenPipe => write!(f, "Broken pipe"),
            VfsError::WouldBlock => write!(f, "Operation would block"),
            VfsError::Interrupted => write!(f, "Interrupted system call"),
            VfsError::InvalidOperation => write!(f, "Invalid operation"),
            VfsError::InvalidFilesystem => write!(f, "Invalid filesystem"),
            VfsError::CorruptedFilesystem => write!(f, "Corrupted filesystem"),
        }
    }
}

/// VFS result type
pub type VfsResult<T> = Result<T, VfsError>;
