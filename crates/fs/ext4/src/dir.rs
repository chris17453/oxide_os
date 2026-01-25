//! ext4 directory handling

use alloc::string::String;
use alloc::vec::Vec;
use block::BlockDevice;

use crate::error::{Ext4Error, Ext4Result};
use crate::extent::map_block;
use crate::group_desc::read_block;
use crate::inode::{read_inode, Ext4Inode};
use crate::group_desc::BlockGroupTable;
use crate::superblock::Ext4Superblock;

/// Directory entry file types (stored in name_len high byte or file_type field)
pub mod file_type {
    pub const UNKNOWN: u8 = 0;
    pub const REG_FILE: u8 = 1;
    pub const DIR: u8 = 2;
    pub const CHRDEV: u8 = 3;
    pub const BLKDEV: u8 = 4;
    pub const FIFO: u8 = 5;
    pub const SOCK: u8 = 6;
    pub const SYMLINK: u8 = 7;
}

/// ext4 directory entry structure (classic linear format)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Ext4DirEntry {
    /// Inode number
    pub inode: u32,
    /// Directory entry length
    pub rec_len: u16,
    /// Name length
    pub name_len: u8,
    /// File type (if FILETYPE feature enabled)
    pub file_type: u8,
    // Name follows (variable length, not part of struct)
}

/// Parsed directory entry
#[derive(Debug, Clone)]
pub struct DirEntry {
    /// Inode number
    pub inode: u32,
    /// Entry name
    pub name: String,
    /// File type
    pub file_type: u8,
}

/// Iterator over directory entries
pub struct DirIterator<'a> {
    device: &'a dyn BlockDevice,
    sb: &'a Ext4Superblock,
    group_table: &'a BlockGroupTable,
    inode: Ext4Inode,
    /// Current position in directory (byte offset)
    pos: u64,
    /// Directory size
    size: u64,
    /// Current block data
    block_buf: Vec<u8>,
    /// Current block number (logical)
    current_block: u64,
    /// Whether current block is loaded
    block_loaded: bool,
}

impl<'a> DirIterator<'a> {
    /// Create a new directory iterator
    pub fn new(
        device: &'a dyn BlockDevice,
        sb: &'a Ext4Superblock,
        group_table: &'a BlockGroupTable,
        inode: Ext4Inode,
    ) -> Self {
        let size = inode.size();
        let block_size = sb.block_size() as usize;

        DirIterator {
            device,
            sb,
            group_table,
            inode,
            pos: 0,
            size,
            block_buf: alloc::vec![0u8; block_size],
            current_block: u64::MAX,
            block_loaded: false,
        }
    }

    /// Load block at given logical block number
    fn load_block(&mut self, logical_block: u64) -> Ext4Result<bool> {
        if self.current_block == logical_block && self.block_loaded {
            return Ok(true);
        }

        // Map logical to physical block
        let phys = match map_block(self.device, self.sb, &self.inode, logical_block)? {
            Some(p) => p,
            None => return Ok(false), // Sparse - treat as empty
        };

        // Read the block
        read_block(self.device, self.sb, phys, &mut self.block_buf)?;
        self.current_block = logical_block;
        self.block_loaded = true;
        Ok(true)
    }

    /// Read next directory entry
    pub fn next_entry(&mut self) -> Ext4Result<Option<DirEntry>> {
        let block_size = self.sb.block_size();

        while self.pos < self.size {
            let logical_block = self.pos / block_size;
            let offset_in_block = (self.pos % block_size) as usize;

            // Load the block if needed
            if !self.load_block(logical_block)? {
                // Sparse block - skip to next
                self.pos = (logical_block + 1) * block_size;
                continue;
            }

            // Check if we have enough data for the header
            if offset_in_block + 8 > self.block_buf.len() {
                self.pos = (logical_block + 1) * block_size;
                continue;
            }

            // Parse entry header
            let entry: Ext4DirEntry = unsafe {
                core::ptr::read_unaligned(
                    self.block_buf[offset_in_block..].as_ptr() as *const Ext4DirEntry
                )
            };

            // Validate rec_len
            if entry.rec_len < 8 || entry.rec_len as usize > block_size as usize - offset_in_block {
                return Err(Ext4Error::InvalidDirEntry);
            }

            // Move to next entry
            self.pos += entry.rec_len as u64;

            // Skip deleted entries (inode == 0)
            if entry.inode == 0 {
                continue;
            }

            // Extract name
            let name_start = offset_in_block + 8;
            let name_end = name_start + entry.name_len as usize;

            if name_end > self.block_buf.len() {
                return Err(Ext4Error::InvalidDirEntry);
            }

            let name_bytes = &self.block_buf[name_start..name_end];
            let name = String::from_utf8_lossy(name_bytes).into_owned();

            return Ok(Some(DirEntry {
                inode: entry.inode,
                name,
                file_type: entry.file_type,
            }));
        }

        Ok(None)
    }
}

/// Look up a name in a directory
pub fn lookup(
    device: &dyn BlockDevice,
    sb: &Ext4Superblock,
    group_table: &BlockGroupTable,
    dir_inode: &Ext4Inode,
    name: &str,
) -> Ext4Result<Option<u32>> {
    if !dir_inode.is_dir() {
        return Err(Ext4Error::NotDirectory);
    }

    let mut iter = DirIterator::new(device, sb, group_table, *dir_inode);

    while let Some(entry) = iter.next_entry()? {
        if entry.name == name {
            return Ok(Some(entry.inode));
        }
    }

    Ok(None)
}

/// Read all directory entries
pub fn readdir(
    device: &dyn BlockDevice,
    sb: &Ext4Superblock,
    group_table: &BlockGroupTable,
    dir_inode: &Ext4Inode,
) -> Ext4Result<Vec<DirEntry>> {
    if !dir_inode.is_dir() {
        return Err(Ext4Error::NotDirectory);
    }

    let mut entries = Vec::new();
    let mut iter = DirIterator::new(device, sb, group_table, *dir_inode);

    while let Some(entry) = iter.next_entry()? {
        entries.push(entry);
    }

    Ok(entries)
}

/// Get directory entry at a specific offset (for readdir with offset support)
pub fn readdir_at(
    device: &dyn BlockDevice,
    sb: &Ext4Superblock,
    group_table: &BlockGroupTable,
    dir_inode: &Ext4Inode,
    offset: u64,
) -> Ext4Result<Option<(DirEntry, u64)>> {
    if !dir_inode.is_dir() {
        return Err(Ext4Error::NotDirectory);
    }

    let mut iter = DirIterator::new(device, sb, group_table, *dir_inode);

    // Skip to offset
    let mut current_offset = 0u64;
    while current_offset < offset {
        match iter.next_entry()? {
            Some(_) => current_offset += 1,
            None => return Ok(None),
        }
    }

    // Get the entry at offset
    match iter.next_entry()? {
        Some(entry) => Ok(Some((entry, current_offset + 1))),
        None => Ok(None),
    }
}

/// Convert ext4 directory file type to VFS VnodeType
pub fn file_type_to_vnode_type(ft: u8) -> vfs::VnodeType {
    match ft {
        file_type::REG_FILE => vfs::VnodeType::File,
        file_type::DIR => vfs::VnodeType::Directory,
        file_type::SYMLINK => vfs::VnodeType::Symlink,
        file_type::CHRDEV => vfs::VnodeType::CharDevice,
        file_type::BLKDEV => vfs::VnodeType::BlockDevice,
        file_type::FIFO => vfs::VnodeType::Fifo,
        file_type::SOCK => vfs::VnodeType::Socket,
        _ => vfs::VnodeType::File,
    }
}
