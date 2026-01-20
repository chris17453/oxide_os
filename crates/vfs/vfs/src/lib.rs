//! OXIDE Virtual Filesystem
//!
//! Provides the VFS abstraction layer for filesystem operations.

#![no_std]

extern crate alloc;

pub mod vnode;
pub mod file;
pub mod path;
pub mod mount;
pub mod error;
pub mod fd;
pub mod pipe;

pub use error::{VfsError, VfsResult};
pub use vnode::{DirEntry, Vnode, VnodeOps, VnodeType, Stat, Mode};
pub use file::{File, FileFlags, SeekFrom};
pub use path::Path;
pub use mount::{Mount, MountFlags, VFS};
pub use fd::{FileDescriptor, FdTable};
