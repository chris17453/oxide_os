//! ext4 block and inode bitmap handling
//!
//! Provides allocation and deallocation of blocks and inodes using bitmaps.

use alloc::vec;
use block::BlockDevice;

use crate::error::{Ext4Error, Ext4Result};
use crate::group_desc::{BlockGroupDesc, BlockGroupTable, read_block, write_block};
use crate::superblock::Ext4Superblock;

/// Block bitmap operations
pub struct BlockBitmap {
    /// Block group number
    group: u32,
    /// Bitmap data (one block)
    data: alloc::vec::Vec<u8>,
    /// Whether the bitmap has been modified
    dirty: bool,
}

impl BlockBitmap {
    /// Read block bitmap for a group
    pub fn read(
        device: &dyn BlockDevice,
        sb: &Ext4Superblock,
        group_table: &BlockGroupTable,
        group: u32,
    ) -> Ext4Result<Self> {
        let desc = group_table.get(group).ok_or(Ext4Error::InvalidGroupDesc)?;
        let block_size = sb.block_size() as usize;

        let mut data = vec![0u8; block_size];
        read_block(device, sb, desc.block_bitmap, &mut data)?;

        Ok(BlockBitmap {
            group,
            data,
            dirty: false,
        })
    }

    /// Write block bitmap back to disk
    pub fn write(
        &self,
        device: &dyn BlockDevice,
        sb: &Ext4Superblock,
        group_table: &BlockGroupTable,
    ) -> Ext4Result<()> {
        if !self.dirty {
            return Ok(());
        }

        let desc = group_table
            .get(self.group)
            .ok_or(Ext4Error::InvalidGroupDesc)?;
        write_block(device, sb, desc.block_bitmap, &self.data)?;
        Ok(())
    }

    /// Check if a bit (block within group) is set (allocated)
    pub fn is_set(&self, bit: u32) -> bool {
        let byte_idx = (bit / 8) as usize;
        let bit_idx = bit % 8;

        if byte_idx >= self.data.len() {
            return true; // Out of range = allocated
        }

        (self.data[byte_idx] & (1 << bit_idx)) != 0
    }

    /// Set a bit (mark block as allocated)
    pub fn set(&mut self, bit: u32) {
        let byte_idx = (bit / 8) as usize;
        let bit_idx = bit % 8;

        if byte_idx < self.data.len() {
            self.data[byte_idx] |= 1 << bit_idx;
            self.dirty = true;
        }
    }

    /// Clear a bit (mark block as free)
    pub fn clear(&mut self, bit: u32) {
        let byte_idx = (bit / 8) as usize;
        let bit_idx = bit % 8;

        if byte_idx < self.data.len() {
            self.data[byte_idx] &= !(1 << bit_idx);
            self.dirty = true;
        }
    }

    /// Find first free bit in bitmap
    pub fn find_first_free(&self, max_bits: u32) -> Option<u32> {
        for bit in 0..max_bits {
            if !self.is_set(bit) {
                return Some(bit);
            }
        }
        None
    }

    /// Find first free range of consecutive bits
    pub fn find_free_range(&self, count: u32, max_bits: u32) -> Option<u32> {
        if count == 0 {
            return Some(0);
        }

        let mut start = 0u32;
        let mut found = 0u32;

        for bit in 0..max_bits {
            if !self.is_set(bit) {
                if found == 0 {
                    start = bit;
                }
                found += 1;
                if found >= count {
                    return Some(start);
                }
            } else {
                found = 0;
            }
        }
        None
    }

    /// Count free bits
    pub fn count_free(&self, max_bits: u32) -> u32 {
        let mut count = 0;
        for bit in 0..max_bits {
            if !self.is_set(bit) {
                count += 1;
            }
        }
        count
    }
}

/// Inode bitmap operations
pub struct InodeBitmap {
    /// Block group number
    group: u32,
    /// Bitmap data
    data: alloc::vec::Vec<u8>,
    /// Whether the bitmap has been modified
    dirty: bool,
}

impl InodeBitmap {
    /// Read inode bitmap for a group
    pub fn read(
        device: &dyn BlockDevice,
        sb: &Ext4Superblock,
        group_table: &BlockGroupTable,
        group: u32,
    ) -> Ext4Result<Self> {
        let desc = group_table.get(group).ok_or(Ext4Error::InvalidGroupDesc)?;
        let block_size = sb.block_size() as usize;

        let mut data = vec![0u8; block_size];
        read_block(device, sb, desc.inode_bitmap, &mut data)?;

        Ok(InodeBitmap {
            group,
            data,
            dirty: false,
        })
    }

    /// Write inode bitmap back to disk
    pub fn write(
        &self,
        device: &dyn BlockDevice,
        sb: &Ext4Superblock,
        group_table: &BlockGroupTable,
    ) -> Ext4Result<()> {
        if !self.dirty {
            return Ok(());
        }

        let desc = group_table
            .get(self.group)
            .ok_or(Ext4Error::InvalidGroupDesc)?;
        write_block(device, sb, desc.inode_bitmap, &self.data)?;
        Ok(())
    }

    /// Check if a bit (inode within group) is set (allocated)
    pub fn is_set(&self, bit: u32) -> bool {
        let byte_idx = (bit / 8) as usize;
        let bit_idx = bit % 8;

        if byte_idx >= self.data.len() {
            return true;
        }

        (self.data[byte_idx] & (1 << bit_idx)) != 0
    }

    /// Set a bit (mark inode as allocated)
    pub fn set(&mut self, bit: u32) {
        let byte_idx = (bit / 8) as usize;
        let bit_idx = bit % 8;

        if byte_idx < self.data.len() {
            self.data[byte_idx] |= 1 << bit_idx;
            self.dirty = true;
        }
    }

    /// Clear a bit (mark inode as free)
    pub fn clear(&mut self, bit: u32) {
        let byte_idx = (bit / 8) as usize;
        let bit_idx = bit % 8;

        if byte_idx < self.data.len() {
            self.data[byte_idx] &= !(1 << bit_idx);
            self.dirty = true;
        }
    }

    /// Find first free inode in bitmap
    pub fn find_first_free(&self, max_bits: u32) -> Option<u32> {
        for bit in 0..max_bits {
            if !self.is_set(bit) {
                return Some(bit);
            }
        }
        None
    }
}

/// Allocate a block from a specific group
pub fn alloc_block_in_group(
    device: &dyn BlockDevice,
    sb: &Ext4Superblock,
    group_table: &BlockGroupTable,
    group: u32,
) -> Ext4Result<Option<u64>> {
    let desc = group_table.get(group).ok_or(Ext4Error::InvalidGroupDesc)?;

    // Check if group has free blocks
    if desc.free_blocks_count == 0 {
        return Ok(None);
    }

    let mut bitmap = BlockBitmap::read(device, sb, group_table, group)?;
    let blocks_per_group = sb.s_blocks_per_group;

    // Find a free block
    if let Some(bit) = bitmap.find_first_free(blocks_per_group) {
        bitmap.set(bit);
        bitmap.write(device, sb, group_table)?;

        // Calculate absolute block number
        let block_num = (group as u64) * (blocks_per_group as u64) + (bit as u64);

        // Skip block 0 (superblock resides in first block for 1k blocks)
        if block_num == 0 {
            return alloc_block_in_group(device, sb, group_table, group);
        }

        return Ok(Some(block_num));
    }

    Ok(None)
}

/// Allocate a block (searches all groups)
pub fn alloc_block(
    device: &dyn BlockDevice,
    sb: &Ext4Superblock,
    group_table: &BlockGroupTable,
    preferred_group: Option<u32>,
) -> Ext4Result<Option<u64>> {
    let num_groups = sb.block_group_count();

    // Try preferred group first
    if let Some(group) = preferred_group {
        if group < num_groups {
            if let Some(block) = alloc_block_in_group(device, sb, group_table, group)? {
                return Ok(Some(block));
            }
        }
    }

    // Search all groups
    for group in 0..num_groups {
        if let Some(block) = alloc_block_in_group(device, sb, group_table, group)? {
            return Ok(Some(block));
        }
    }

    Ok(None)
}

/// Free a block
pub fn free_block(
    device: &dyn BlockDevice,
    sb: &Ext4Superblock,
    group_table: &BlockGroupTable,
    block_num: u64,
) -> Ext4Result<()> {
    let blocks_per_group = sb.s_blocks_per_group as u64;
    let group = (block_num / blocks_per_group) as u32;
    let bit = (block_num % blocks_per_group) as u32;

    let mut bitmap = BlockBitmap::read(device, sb, group_table, group)?;
    bitmap.clear(bit);
    bitmap.write(device, sb, group_table)?;

    Ok(())
}

/// Allocate an inode from a specific group
pub fn alloc_inode_in_group(
    device: &dyn BlockDevice,
    sb: &Ext4Superblock,
    group_table: &BlockGroupTable,
    group: u32,
) -> Ext4Result<Option<u32>> {
    let desc = group_table.get(group).ok_or(Ext4Error::InvalidGroupDesc)?;

    // Check if group has free inodes
    if desc.free_inodes_count == 0 {
        return Ok(None);
    }

    let mut bitmap = InodeBitmap::read(device, sb, group_table, group)?;
    let inodes_per_group = sb.s_inodes_per_group;

    // Find a free inode
    if let Some(bit) = bitmap.find_first_free(inodes_per_group) {
        bitmap.set(bit);
        bitmap.write(device, sb, group_table)?;

        // Calculate inode number (1-based)
        let ino = group * inodes_per_group + bit + 1;

        // Skip reserved inodes (1-10)
        if ino <= 10 {
            // Try the next one
            return alloc_inode_in_group(device, sb, group_table, group);
        }

        return Ok(Some(ino));
    }

    Ok(None)
}

/// Allocate an inode (searches all groups)
pub fn alloc_inode(
    device: &dyn BlockDevice,
    sb: &Ext4Superblock,
    group_table: &BlockGroupTable,
    preferred_group: Option<u32>,
    is_directory: bool,
) -> Ext4Result<Option<u32>> {
    let num_groups = sb.block_group_count();

    // For directories, use Orlov allocator heuristic (simplified):
    // Try to spread directories across groups
    if is_directory {
        // Find group with most free inodes
        let mut best_group = 0u32;
        let mut best_free = 0u32;

        for group in 0..num_groups {
            if let Some(desc) = group_table.get(group) {
                if desc.free_inodes_count > best_free {
                    best_free = desc.free_inodes_count;
                    best_group = group;
                }
            }
        }

        if let Some(ino) = alloc_inode_in_group(device, sb, group_table, best_group)? {
            return Ok(Some(ino));
        }
    }

    // Try preferred group first (usually parent directory's group)
    if let Some(group) = preferred_group {
        if group < num_groups {
            if let Some(ino) = alloc_inode_in_group(device, sb, group_table, group)? {
                return Ok(Some(ino));
            }
        }
    }

    // Search all groups
    for group in 0..num_groups {
        if let Some(ino) = alloc_inode_in_group(device, sb, group_table, group)? {
            return Ok(Some(ino));
        }
    }

    Ok(None)
}

/// Free an inode
pub fn free_inode(
    device: &dyn BlockDevice,
    sb: &Ext4Superblock,
    group_table: &BlockGroupTable,
    ino: u32,
) -> Ext4Result<()> {
    if ino == 0 {
        return Err(Ext4Error::InvalidInode);
    }

    let inodes_per_group = sb.s_inodes_per_group;
    let group = (ino - 1) / inodes_per_group;
    let bit = (ino - 1) % inodes_per_group;

    let mut bitmap = InodeBitmap::read(device, sb, group_table, group)?;
    bitmap.clear(bit);
    bitmap.write(device, sb, group_table)?;

    Ok(())
}

/// Allocate multiple consecutive blocks
pub fn alloc_blocks(
    device: &dyn BlockDevice,
    sb: &Ext4Superblock,
    group_table: &BlockGroupTable,
    count: u32,
    preferred_group: Option<u32>,
) -> Ext4Result<Option<u64>> {
    let num_groups = sb.block_group_count();
    let blocks_per_group = sb.s_blocks_per_group;

    // Try preferred group first
    if let Some(group) = preferred_group {
        if group < num_groups {
            let mut bitmap = BlockBitmap::read(device, sb, group_table, group)?;
            if let Some(bit) = bitmap.find_free_range(count, blocks_per_group) {
                // Mark all blocks as allocated
                for i in 0..count {
                    bitmap.set(bit + i);
                }
                bitmap.write(device, sb, group_table)?;

                let block_num = (group as u64) * (blocks_per_group as u64) + (bit as u64);
                return Ok(Some(block_num));
            }
        }
    }

    // Search all groups
    for group in 0..num_groups {
        let mut bitmap = BlockBitmap::read(device, sb, group_table, group)?;
        if let Some(bit) = bitmap.find_free_range(count, blocks_per_group) {
            for i in 0..count {
                bitmap.set(bit + i);
            }
            bitmap.write(device, sb, group_table)?;

            let block_num = (group as u64) * (blocks_per_group as u64) + (bit as u64);
            return Ok(Some(block_num));
        }
    }

    Ok(None)
}
