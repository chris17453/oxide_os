//! OXIDE Virtual Filesystem
//!
//! Provides the VFS abstraction layer for filesystem operations.

#![no_std]

extern crate alloc;

pub mod error;
pub mod fd;
pub mod file;
pub mod mount;
pub mod path;
pub mod pipe;
pub mod vnode;

pub use error::{VfsError, VfsResult};
pub use fd::{FdTable, FileDescriptor};
pub use file::{File, FileFlags, SeekFrom};
pub use mount::{Mount, MountFlags, VFS};
pub use path::Path;
pub use vnode::{DirEntry, Mode, Stat, Vnode, VnodeOps, VnodeType};
