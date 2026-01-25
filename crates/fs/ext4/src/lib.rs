//! ext4 Filesystem Implementation for OXIDE OS
//!
//! Provides read support for ext4 filesystems with:
//! - Superblock parsing and validation
//! - Block group descriptor handling
//! - Inode reading with extent tree support
//! - Directory traversal
//! - File reading
//! - VFS integration via VnodeOps

#![no_std]
#![allow(unused)]

extern crate alloc;

pub mod dir;
pub mod error;
pub mod extent;
pub mod file;
pub mod group_desc;
pub mod inode;
pub mod superblock;
pub mod vnode;

use alloc::sync::Arc;
use spin::RwLock;

use block::BlockDevice;
use vfs::{VfsError, VfsResult, VnodeOps};

use error::{Ext4Error, Ext4Result};
use group_desc::BlockGroupTable;
use inode::{ino, read_inode};
use superblock::Ext4Superblock;
use vnode::{Ext4Fs, Ext4Vnode};

/// Mount an ext4 filesystem from a block device
///
/// Returns the root vnode of the mounted filesystem.
pub fn mount(device: Arc<dyn BlockDevice>, read_only: bool) -> VfsResult<Arc<dyn VnodeOps>> {
    // Read and validate superblock
    let sb = Ext4Superblock::read(&*device).map_err(|e| VfsError::from(e))?;

    // Validate features
    sb.validate().map_err(|e| VfsError::from(e))?;

    // Read block group descriptor table
    let group_table = BlockGroupTable::read(&*device, &sb).map_err(|e| VfsError::from(e))?;

    // Create filesystem state
    let fs = Arc::new(RwLock::new(Ext4Fs {
        device,
        sb,
        group_table,
        read_only,
    }));

    // Read root inode (always inode 2)
    let root_inode = {
        let fs_guard = fs.read();
        read_inode(
            fs_guard.device(),
            &fs_guard.sb,
            &fs_guard.group_table,
            ino::ROOT_INO,
        )
        .map_err(|e| VfsError::from(e))?
    };

    // Create root vnode
    let root = Arc::new(Ext4Vnode::new(fs, ino::ROOT_INO, root_inode));

    Ok(root)
}

/// Get filesystem information
pub struct Ext4Info {
    /// Total blocks
    pub blocks_total: u64,
    /// Free blocks
    pub blocks_free: u64,
    /// Block size
    pub block_size: u64,
    /// Total inodes
    pub inodes_total: u32,
    /// Free inodes
    pub inodes_free: u32,
    /// Volume name (if set)
    pub volume_name: [u8; 16],
    /// Has journal
    pub has_journal: bool,
    /// Uses extents
    pub has_extents: bool,
    /// 64-bit mode
    pub is_64bit: bool,
}

/// Get filesystem info from superblock
pub fn get_info(device: &dyn BlockDevice) -> Ext4Result<Ext4Info> {
    let sb = Ext4Superblock::read(device)?;

    Ok(Ext4Info {
        blocks_total: sb.blocks_count(),
        blocks_free: sb.free_blocks_count(),
        block_size: sb.block_size(),
        inodes_total: sb.s_inodes_count,
        inodes_free: sb.s_free_inodes_count,
        volume_name: sb.s_volume_name,
        has_journal: sb.has_journal(),
        has_extents: sb.has_extents(),
        is_64bit: sb.is_64bit(),
    })
}

/// Check if a block device contains an ext4 filesystem
pub fn is_ext4(device: &dyn BlockDevice) -> bool {
    match Ext4Superblock::read(device) {
        Ok(sb) => sb.s_magic == superblock::EXT4_MAGIC,
        Err(_) => false,
    }
}
