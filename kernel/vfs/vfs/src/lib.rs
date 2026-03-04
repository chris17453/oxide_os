//! OXIDE Virtual Filesystem
//!
//! Provides the VFS abstraction layer for filesystem operations.

#![no_std]

extern crate alloc;

pub mod epoll;
pub mod error;
pub mod eventfd;
pub mod fd;
pub mod file;
pub mod flock;
pub mod memfd;
pub mod mount;
pub mod path;
pub mod permission;
pub mod pipe;
pub mod vnode;

pub use error::{VfsError, VfsResult};
pub use fd::{FdTable, FileDescriptor};
pub use file::{File, FileFlags, SeekFrom};
pub use flock::{FLOCK_REGISTRY, FlockRegistry, InodeId};
pub use mount::{FsInfo, Mount, MountFlags, VFS, vfs_statfs};
pub use path::Path;
pub use vnode::{DirEntry, Mode, Stat, Vnode, VnodeOps, VnodeType};
