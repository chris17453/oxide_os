//! ext4 block group descriptor handling

use alloc::vec::Vec;
use block::BlockDevice;

use crate::error::{Ext4Error, Ext4Result};
use crate::superblock::Ext4Superblock;

/// ext4 block group descriptor (32 bytes, classic)
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Ext4GroupDesc32 {
    pub bg_block_bitmap_lo: u32,
    pub bg_inode_bitmap_lo: u32,
    pub bg_inode_table_lo: u32,
    pub bg_free_blocks_count_lo: u16,
    pub bg_free_inodes_count_lo: u16,
    pub bg_used_dirs_count_lo: u16,
    pub bg_flags: u16,
    pub bg_exclude_bitmap_lo: u32,
    pub bg_block_bitmap_csum_lo: u16,
    pub bg_inode_bitmap_csum_lo: u16,
    pub bg_itable_unused_lo: u16,
    pub bg_checksum: u16,
}

/// ext4 block group descriptor (64 bytes, with 64-bit support)
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Ext4GroupDesc64 {
    // 32-byte base
    pub bg_block_bitmap_lo: u32,
    pub bg_inode_bitmap_lo: u32,
    pub bg_inode_table_lo: u32,
    pub bg_free_blocks_count_lo: u16,
    pub bg_free_inodes_count_lo: u16,
    pub bg_used_dirs_count_lo: u16,
    pub bg_flags: u16,
    pub bg_exclude_bitmap_lo: u32,
    pub bg_block_bitmap_csum_lo: u16,
    pub bg_inode_bitmap_csum_lo: u16,
    pub bg_itable_unused_lo: u16,
    pub bg_checksum: u16,
    // 64-bit extension
    pub bg_block_bitmap_hi: u32,
    pub bg_inode_bitmap_hi: u32,
    pub bg_inode_table_hi: u32,
    pub bg_free_blocks_count_hi: u16,
    pub bg_free_inodes_count_hi: u16,
    pub bg_used_dirs_count_hi: u16,
    pub bg_itable_unused_hi: u16,
    pub bg_exclude_bitmap_hi: u32,
    pub bg_block_bitmap_csum_hi: u16,
    pub bg_inode_bitmap_csum_hi: u16,
    pub bg_reserved: u32,
}

/// Unified block group descriptor (supports both 32 and 64-bit)
#[derive(Debug, Clone, Copy)]
pub struct BlockGroupDesc {
    pub block_bitmap: u64,
    pub inode_bitmap: u64,
    pub inode_table: u64,
    pub free_blocks_count: u32,
    pub free_inodes_count: u32,
    pub used_dirs_count: u32,
    pub flags: u16,
}

impl BlockGroupDesc {
    /// Create from 32-byte descriptor
    pub fn from_32(desc: &Ext4GroupDesc32) -> Self {
        BlockGroupDesc {
            block_bitmap: desc.bg_block_bitmap_lo as u64,
            inode_bitmap: desc.bg_inode_bitmap_lo as u64,
            inode_table: desc.bg_inode_table_lo as u64,
            free_blocks_count: desc.bg_free_blocks_count_lo as u32,
            free_inodes_count: desc.bg_free_inodes_count_lo as u32,
            used_dirs_count: desc.bg_used_dirs_count_lo as u32,
            flags: desc.bg_flags,
        }
    }

    /// Create from 64-byte descriptor
    pub fn from_64(desc: &Ext4GroupDesc64) -> Self {
        BlockGroupDesc {
            block_bitmap: desc.bg_block_bitmap_lo as u64
                | ((desc.bg_block_bitmap_hi as u64) << 32),
            inode_bitmap: desc.bg_inode_bitmap_lo as u64
                | ((desc.bg_inode_bitmap_hi as u64) << 32),
            inode_table: desc.bg_inode_table_lo as u64
                | ((desc.bg_inode_table_hi as u64) << 32),
            free_blocks_count: desc.bg_free_blocks_count_lo as u32
                | ((desc.bg_free_blocks_count_hi as u32) << 16),
            free_inodes_count: desc.bg_free_inodes_count_lo as u32
                | ((desc.bg_free_inodes_count_hi as u32) << 16),
            used_dirs_count: desc.bg_used_dirs_count_lo as u32
                | ((desc.bg_used_dirs_count_hi as u32) << 16),
            flags: desc.bg_flags,
        }
    }
}

/// Block group descriptor table
pub struct BlockGroupTable {
    pub descs: Vec<BlockGroupDesc>,
}

impl BlockGroupTable {
    /// Read block group descriptor table from device
    pub fn read(device: &dyn BlockDevice, sb: &Ext4Superblock) -> Ext4Result<Self> {
        let block_size = sb.block_size();
        let desc_size = sb.desc_size();
        let num_groups = sb.block_group_count();
        let desc_block = sb.group_desc_block();
        let is_64bit = sb.is_64bit();

        // Calculate how many descriptors fit in one block
        let descs_per_block = block_size as u32 / desc_size;

        // Calculate total bytes needed
        let total_bytes = (num_groups as u64) * (desc_size as u64);
        let blocks_needed = (total_bytes + block_size - 1) / block_size;

        // Read all blocks containing descriptors
        let mut buf = alloc::vec![0u8; (blocks_needed * block_size) as usize];

        for i in 0..blocks_needed {
            let block_num = desc_block + i;
            let offset = (i * block_size) as usize;
            read_block(device, sb, block_num, &mut buf[offset..offset + block_size as usize])?;
        }

        // Parse descriptors
        let mut descs = Vec::with_capacity(num_groups as usize);

        for i in 0..num_groups {
            let offset = (i as usize) * (desc_size as usize);
            let desc = if is_64bit && desc_size >= 64 {
                let d: Ext4GroupDesc64 = unsafe {
                    core::ptr::read_unaligned(buf[offset..].as_ptr() as *const Ext4GroupDesc64)
                };
                BlockGroupDesc::from_64(&d)
            } else {
                let d: Ext4GroupDesc32 = unsafe {
                    core::ptr::read_unaligned(buf[offset..].as_ptr() as *const Ext4GroupDesc32)
                };
                BlockGroupDesc::from_32(&d)
            };
            descs.push(desc);
        }

        Ok(BlockGroupTable { descs })
    }

    /// Get descriptor for a block group
    pub fn get(&self, group: u32) -> Option<&BlockGroupDesc> {
        self.descs.get(group as usize)
    }
}

/// Read a filesystem block from the device
pub fn read_block(
    device: &dyn BlockDevice,
    sb: &Ext4Superblock,
    block_num: u64,
    buf: &mut [u8],
) -> Ext4Result<()> {
    let block_size = sb.block_size();
    let sector_size = 512u64;
    let sectors_per_block = block_size / sector_size;
    let start_sector = block_num * sectors_per_block;

    // Read all sectors for this block
    for i in 0..sectors_per_block {
        let sector = start_sector + i;
        let offset = (i * sector_size) as usize;
        device
            .read(sector, &mut buf[offset..offset + sector_size as usize])
            .map_err(|_| Ext4Error::IoError)?;
    }

    Ok(())
}

/// Write a filesystem block to the device
pub fn write_block(
    device: &dyn BlockDevice,
    sb: &Ext4Superblock,
    block_num: u64,
    buf: &[u8],
) -> Ext4Result<()> {
    let block_size = sb.block_size();
    let sector_size = 512u64;
    let sectors_per_block = block_size / sector_size;
    let start_sector = block_num * sectors_per_block;

    // Write all sectors for this block
    for i in 0..sectors_per_block {
        let sector = start_sector + i;
        let offset = (i * sector_size) as usize;
        device
            .write(sector, &buf[offset..offset + sector_size as usize])
            .map_err(|_| Ext4Error::IoError)?;
    }

    Ok(())
}
