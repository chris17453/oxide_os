//! ext4 filesystem errors

use block::BlockError;
use vfs::VfsError;

/// ext4-specific error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ext4Error {
    /// Invalid superblock magic number
    InvalidMagic,
    /// Unsupported filesystem feature
    UnsupportedFeature,
    /// Invalid block group descriptor
    InvalidGroupDesc,
    /// Invalid inode number
    InvalidInode,
    /// Invalid extent header
    InvalidExtent,
    /// Invalid directory entry
    InvalidDirEntry,
    /// Block device I/O error
    IoError,
    /// Out of space
    NoSpace,
    /// Filesystem is read-only
    ReadOnly,
    /// Not a directory
    NotDirectory,
    /// Is a directory
    IsDirectory,
    /// Not found
    NotFound,
    /// Already exists
    AlreadyExists,
    /// Directory not empty
    NotEmpty,
    /// Name too long
    NameTooLong,
    /// Corrupt filesystem
    Corrupt,
    /// Journal error
    JournalError,
}

impl From<BlockError> for Ext4Error {
    fn from(_: BlockError) -> Self {
        Ext4Error::IoError
    }
}

impl From<Ext4Error> for VfsError {
    fn from(e: Ext4Error) -> Self {
        match e {
            Ext4Error::InvalidMagic => VfsError::IoError,
            Ext4Error::UnsupportedFeature => VfsError::NotSupported,
            Ext4Error::InvalidGroupDesc => VfsError::IoError,
            Ext4Error::InvalidInode => VfsError::IoError,
            Ext4Error::InvalidExtent => VfsError::IoError,
            Ext4Error::InvalidDirEntry => VfsError::IoError,
            Ext4Error::IoError => VfsError::IoError,
            Ext4Error::NoSpace => VfsError::NoSpace,
            Ext4Error::ReadOnly => VfsError::ReadOnly,
            Ext4Error::NotDirectory => VfsError::NotDirectory,
            Ext4Error::IsDirectory => VfsError::IsDirectory,
            Ext4Error::NotFound => VfsError::NotFound,
            Ext4Error::AlreadyExists => VfsError::AlreadyExists,
            Ext4Error::NotEmpty => VfsError::NotEmpty,
            Ext4Error::NameTooLong => VfsError::NameTooLong,
            Ext4Error::Corrupt => VfsError::IoError,
            Ext4Error::JournalError => VfsError::IoError,
        }
    }
}

/// Result type for ext4 operations
pub type Ext4Result<T> = Result<T, Ext4Error>;
