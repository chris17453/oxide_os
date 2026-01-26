//! ext4 directory handling

use alloc::string::String;
use alloc::vec::Vec;
use block::BlockDevice;

use crate::error::{Ext4Error, Ext4Result};
use crate::extent::map_block;
use crate::group_desc::BlockGroupTable;
use crate::group_desc::read_block;
use crate::inode::{Ext4Inode, read_inode};
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

/// Convert inode mode to directory file type
pub fn mode_to_file_type(mode: u16) -> u8 {
    use crate::inode::file_type as inode_ft;
    match mode & inode_ft::S_IFMT {
        inode_ft::S_IFREG => file_type::REG_FILE,
        inode_ft::S_IFDIR => file_type::DIR,
        inode_ft::S_IFLNK => file_type::SYMLINK,
        inode_ft::S_IFCHR => file_type::CHRDEV,
        inode_ft::S_IFBLK => file_type::BLKDEV,
        inode_ft::S_IFIFO => file_type::FIFO,
        inode_ft::S_IFSOCK => file_type::SOCK,
        _ => file_type::UNKNOWN,
    }
}

// ============================================================================
// WRITE SUPPORT
// ============================================================================

/// Calculate the actual size needed for a directory entry (with padding)
fn entry_size(name_len: usize) -> usize {
    // Base entry is 8 bytes, name follows, aligned to 4 bytes
    let size = 8 + name_len;
    (size + 3) & !3 // Round up to 4-byte boundary
}

/// Add a directory entry to a directory
///
/// This function searches for free space in existing directory blocks,
/// or allocates a new block if needed.
pub fn add_entry(
    device: &dyn BlockDevice,
    sb: &Ext4Superblock,
    group_table: &BlockGroupTable,
    dir_inode: &mut Ext4Inode,
    name: &str,
    child_ino: u32,
    child_file_type: u8,
) -> Ext4Result<()> {
    use crate::group_desc::write_block;

    if !dir_inode.is_dir() {
        return Err(Ext4Error::NotDirectory);
    }

    if name.len() > 255 {
        return Err(Ext4Error::NameTooLong);
    }

    let block_size = sb.block_size();
    let needed_size = entry_size(name.len());
    let dir_size = dir_inode.size();

    // Search existing blocks for free space
    let num_blocks = (dir_size + block_size - 1) / block_size;
    let mut block_buf = alloc::vec![0u8; block_size as usize];

    for block_idx in 0..num_blocks {
        let logical_block = block_idx;

        // Map logical to physical
        let phys = match map_block(device, sb, dir_inode, logical_block)? {
            Some(p) => p,
            None => continue, // Sparse block
        };

        // Read the block
        read_block(device, sb, phys, &mut block_buf)?;

        // Scan for space in this block
        let mut offset = 0usize;
        while offset < block_size as usize {
            let entry: Ext4DirEntry = unsafe {
                core::ptr::read_unaligned(block_buf[offset..].as_ptr() as *const Ext4DirEntry)
            };

            if entry.rec_len < 8 {
                return Err(Ext4Error::InvalidDirEntry);
            }

            let actual_size = if entry.inode == 0 {
                8 // Deleted entry, minimum header size
            } else {
                entry_size(entry.name_len as usize)
            };

            let free_space = entry.rec_len as usize - actual_size;

            if free_space >= needed_size {
                // Found space! Split this entry.
                if entry.inode != 0 {
                    // Update the current entry's rec_len to its actual size
                    let new_rec_len = actual_size as u16;
                    let entry_bytes = &mut block_buf[offset..];

                    // Write new rec_len (offset 4-5 in entry)
                    entry_bytes[4] = new_rec_len as u8;
                    entry_bytes[5] = (new_rec_len >> 8) as u8;

                    // Insert new entry after
                    let new_offset = offset + actual_size;
                    let new_entry_rec_len = (entry.rec_len as usize - actual_size) as u16;

                    write_dir_entry(
                        &mut block_buf[new_offset..],
                        child_ino,
                        new_entry_rec_len,
                        name,
                        child_file_type,
                    );
                } else {
                    // Reuse this deleted entry
                    write_dir_entry(
                        &mut block_buf[offset..],
                        child_ino,
                        entry.rec_len,
                        name,
                        child_file_type,
                    );
                }

                // Write block back
                write_block(device, sb, phys, &block_buf)?;
                return Ok(());
            }

            offset += entry.rec_len as usize;
        }
    }

    // No space in existing blocks - need to allocate a new block
    allocate_new_dir_block(
        device,
        sb,
        group_table,
        dir_inode,
        name,
        child_ino,
        child_file_type,
    )
}

/// Write a directory entry to a buffer
fn write_dir_entry(buf: &mut [u8], inode: u32, rec_len: u16, name: &str, file_type: u8) {
    // inode (4 bytes)
    buf[0..4].copy_from_slice(&inode.to_le_bytes());
    // rec_len (2 bytes)
    buf[4..6].copy_from_slice(&rec_len.to_le_bytes());
    // name_len (1 byte)
    buf[6] = name.len() as u8;
    // file_type (1 byte)
    buf[7] = file_type;
    // name
    buf[8..8 + name.len()].copy_from_slice(name.as_bytes());
}

/// Allocate a new block for directory entries
fn allocate_new_dir_block(
    device: &dyn BlockDevice,
    sb: &Ext4Superblock,
    group_table: &BlockGroupTable,
    dir_inode: &mut Ext4Inode,
    name: &str,
    child_ino: u32,
    child_file_type: u8,
) -> Ext4Result<()> {
    use crate::bitmap::alloc_block;
    use crate::extent::{insert_extent, try_extend_extent};
    use crate::group_desc::write_block;

    let block_size = sb.block_size();

    // Determine which group to allocate from (prefer same group as directory)
    let dir_group = (dir_inode.i_block[0] as u64 / sb.s_blocks_per_group as u64) as u32;

    // Allocate a new block
    let new_block =
        alloc_block(device, sb, group_table, Some(dir_group))?.ok_or(Ext4Error::NoSpace)?;

    // Calculate logical block number for the new block
    let current_size = dir_inode.size();
    let logical_block = (current_size / block_size) as u32;

    // Try to extend existing extent or insert new one
    if !try_extend_extent(dir_inode, logical_block, new_block)? {
        insert_extent(dir_inode, logical_block, new_block, 1)?;
    }

    // Initialize the new block with the entry
    let mut block_buf = alloc::vec![0u8; block_size as usize];

    // The entry takes up the entire block (rec_len = block_size)
    write_dir_entry(
        &mut block_buf,
        child_ino,
        block_size as u16,
        name,
        child_file_type,
    );

    // Write the block
    write_block(device, sb, new_block, &block_buf)?;

    // Update directory size
    dir_inode.set_size(current_size + block_size);

    // Update block count (in 512-byte units)
    let blocks_512 = dir_inode.blocks() + (block_size / 512);
    dir_inode.set_blocks(blocks_512);

    Ok(())
}

/// Remove a directory entry by name
pub fn remove_entry(
    device: &dyn BlockDevice,
    sb: &Ext4Superblock,
    dir_inode: &Ext4Inode,
    name: &str,
) -> Ext4Result<u32> {
    use crate::group_desc::write_block;

    if !dir_inode.is_dir() {
        return Err(Ext4Error::NotDirectory);
    }

    let block_size = sb.block_size();
    let dir_size = dir_inode.size();
    let num_blocks = (dir_size + block_size - 1) / block_size;
    let mut block_buf = alloc::vec![0u8; block_size as usize];

    for block_idx in 0..num_blocks {
        let phys = match map_block(device, sb, dir_inode, block_idx)? {
            Some(p) => p,
            None => continue,
        };

        read_block(device, sb, phys, &mut block_buf)?;

        let mut offset = 0usize;
        let mut prev_offset: Option<usize> = None;

        while offset < block_size as usize {
            let entry: Ext4DirEntry = unsafe {
                core::ptr::read_unaligned(block_buf[offset..].as_ptr() as *const Ext4DirEntry)
            };

            if entry.rec_len < 8 {
                return Err(Ext4Error::InvalidDirEntry);
            }

            if entry.inode != 0 {
                // Extract name
                let name_start = offset + 8;
                let name_end = name_start + entry.name_len as usize;
                let entry_name = core::str::from_utf8(&block_buf[name_start..name_end])
                    .map_err(|_| Ext4Error::InvalidDirEntry)?;

                if entry_name == name {
                    // Found the entry to remove
                    let removed_inode = entry.inode;

                    if let Some(prev) = prev_offset {
                        // Merge with previous entry by extending its rec_len
                        let prev_entry: Ext4DirEntry = unsafe {
                            core::ptr::read_unaligned(
                                block_buf[prev..].as_ptr() as *const Ext4DirEntry
                            )
                        };

                        let new_rec_len = prev_entry.rec_len + entry.rec_len;
                        block_buf[prev + 4] = new_rec_len as u8;
                        block_buf[prev + 5] = (new_rec_len >> 8) as u8;
                    } else {
                        // First entry in block - just mark as deleted (set inode to 0)
                        block_buf[offset..offset + 4].copy_from_slice(&0u32.to_le_bytes());
                    }

                    // Write block back
                    write_block(device, sb, phys, &block_buf)?;
                    return Ok(removed_inode);
                }
            }

            prev_offset = Some(offset);
            offset += entry.rec_len as usize;
        }
    }

    Err(Ext4Error::NotFound)
}

/// Check if a directory is empty (only . and .. entries)
pub fn is_empty(
    device: &dyn BlockDevice,
    sb: &Ext4Superblock,
    group_table: &BlockGroupTable,
    dir_inode: &Ext4Inode,
) -> Ext4Result<bool> {
    if !dir_inode.is_dir() {
        return Err(Ext4Error::NotDirectory);
    }

    let mut iter = DirIterator::new(device, sb, group_table, *dir_inode);

    while let Some(entry) = iter.next_entry()? {
        if entry.name != "." && entry.name != ".." {
            return Ok(false);
        }
    }

    Ok(true)
}

/// Initialize a new directory with . and .. entries
pub fn init_directory(
    device: &dyn BlockDevice,
    sb: &Ext4Superblock,
    dir_block: u64,
    self_ino: u32,
    parent_ino: u32,
) -> Ext4Result<()> {
    use crate::group_desc::write_block;

    let block_size = sb.block_size();
    let mut block_buf = alloc::vec![0u8; block_size as usize];

    // . entry (self reference)
    let dot_rec_len = 12u16; // Minimum size for "."
    write_dir_entry(
        &mut block_buf[0..],
        self_ino,
        dot_rec_len,
        ".",
        file_type::DIR,
    );

    // .. entry (parent reference) - takes rest of block
    let dotdot_rec_len = (block_size as u16) - dot_rec_len;
    write_dir_entry(
        &mut block_buf[dot_rec_len as usize..],
        parent_ino,
        dotdot_rec_len,
        "..",
        file_type::DIR,
    );

    // Write the block
    write_block(device, sb, dir_block, &block_buf)?;

    Ok(())
}
