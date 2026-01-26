//! ext4 superblock handling

use crate::error::{Ext4Error, Ext4Result};
use block::BlockDevice;

/// ext4 superblock magic number
pub const EXT4_MAGIC: u16 = 0xEF53;

/// ext4 superblock location (byte offset from start)
pub const SUPERBLOCK_OFFSET: u64 = 1024;

/// ext4 feature flags - compatible features
pub mod compat {
    pub const DIR_PREALLOC: u32 = 0x0001;
    pub const IMAGIC_INODES: u32 = 0x0002;
    pub const HAS_JOURNAL: u32 = 0x0004;
    pub const EXT_ATTR: u32 = 0x0008;
    pub const RESIZE_INODE: u32 = 0x0010;
    pub const DIR_INDEX: u32 = 0x0020;
    pub const SPARSE_SUPER2: u32 = 0x0200;
}

/// ext4 feature flags - incompatible features
pub mod incompat {
    pub const COMPRESSION: u32 = 0x0001;
    pub const FILETYPE: u32 = 0x0002;
    pub const RECOVER: u32 = 0x0004;
    pub const JOURNAL_DEV: u32 = 0x0008;
    pub const META_BG: u32 = 0x0010;
    pub const EXTENTS: u32 = 0x0040;
    pub const _64BIT: u32 = 0x0080;
    pub const MMP: u32 = 0x0100;
    pub const FLEX_BG: u32 = 0x0200;
    pub const EA_INODE: u32 = 0x0400;
    pub const DIRDATA: u32 = 0x1000;
    pub const CSUM_SEED: u32 = 0x2000;
    pub const LARGEDIR: u32 = 0x4000;
    pub const INLINE_DATA: u32 = 0x8000;
    pub const ENCRYPT: u32 = 0x10000;
}

/// ext4 feature flags - read-only compatible features
pub mod ro_compat {
    pub const SPARSE_SUPER: u32 = 0x0001;
    pub const LARGE_FILE: u32 = 0x0002;
    pub const BTREE_DIR: u32 = 0x0004;
    pub const HUGE_FILE: u32 = 0x0008;
    pub const GDT_CSUM: u32 = 0x0010;
    pub const DIR_NLINK: u32 = 0x0020;
    pub const EXTRA_ISIZE: u32 = 0x0040;
    pub const QUOTA: u32 = 0x0100;
    pub const BIGALLOC: u32 = 0x0200;
    pub const METADATA_CSUM: u32 = 0x0400;
    pub const READONLY: u32 = 0x1000;
    pub const PROJECT: u32 = 0x2000;
}

/// ext4 superblock structure (first 256 bytes)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Ext4Superblock {
    // 0x00
    pub s_inodes_count: u32,
    pub s_blocks_count_lo: u32,
    pub s_r_blocks_count_lo: u32,
    pub s_free_blocks_count_lo: u32,
    // 0x10
    pub s_free_inodes_count: u32,
    pub s_first_data_block: u32,
    pub s_log_block_size: u32,
    pub s_log_cluster_size: u32,
    // 0x20
    pub s_blocks_per_group: u32,
    pub s_clusters_per_group: u32,
    pub s_inodes_per_group: u32,
    pub s_mtime: u32,
    // 0x30
    pub s_wtime: u32,
    pub s_mnt_count: u16,
    pub s_max_mnt_count: u16,
    pub s_magic: u16,
    pub s_state: u16,
    pub s_errors: u16,
    pub s_minor_rev_level: u16,
    // 0x40
    pub s_lastcheck: u32,
    pub s_checkinterval: u32,
    pub s_creator_os: u32,
    pub s_rev_level: u32,
    // 0x50
    pub s_def_resuid: u16,
    pub s_def_resgid: u16,
    // Dynamic revision fields (rev_level >= 1)
    pub s_first_ino: u32,
    pub s_inode_size: u16,
    pub s_block_group_nr: u16,
    pub s_feature_compat: u32,
    // 0x60
    pub s_feature_incompat: u32,
    pub s_feature_ro_compat: u32,
    pub s_uuid: [u8; 16],
    // 0x78
    pub s_volume_name: [u8; 16],
    // 0x88
    pub s_last_mounted: [u8; 64],
    // 0xC8
    pub s_algorithm_usage_bitmap: u32,
    // Performance hints
    pub s_prealloc_blocks: u8,
    pub s_prealloc_dir_blocks: u8,
    pub s_reserved_gdt_blocks: u16,
    // 0xD0 - Journal info
    pub s_journal_uuid: [u8; 16],
    // 0xE0
    pub s_journal_inum: u32,
    pub s_journal_dev: u32,
    pub s_last_orphan: u32,
    pub s_hash_seed: [u32; 4],
    // 0xFC
    pub s_def_hash_version: u8,
    pub s_jnl_backup_type: u8,
    pub s_desc_size: u16,
    // 0x100
    pub s_default_mount_opts: u32,
    pub s_first_meta_bg: u32,
    pub s_mkfs_time: u32,
    pub s_jnl_blocks: [u32; 17],
    // 0x150 - 64-bit support
    pub s_blocks_count_hi: u32,
    pub s_r_blocks_count_hi: u32,
    pub s_free_blocks_count_hi: u32,
    pub s_min_extra_isize: u16,
    pub s_want_extra_isize: u16,
    pub s_flags: u32,
    pub s_raid_stride: u16,
    pub s_mmp_interval: u16,
    pub s_mmp_block: u64,
    pub s_raid_stripe_width: u32,
    pub s_log_groups_per_flex: u8,
    pub s_checksum_type: u8,
    pub s_reserved_pad: u16,
    pub s_kbytes_written: u64,
    // ... more fields follow but we don't need all of them
}

impl Ext4Superblock {
    /// Read superblock from a block device
    pub fn read(device: &dyn BlockDevice) -> Ext4Result<Self> {
        let mut buf = [0u8; 1024];

        // Read blocks containing superblock (offset 1024, sector 2-3)
        device
            .read(2, &mut buf[..512])
            .map_err(|_| Ext4Error::IoError)?;
        device
            .read(3, &mut buf[512..])
            .map_err(|_| Ext4Error::IoError)?;

        // Superblock starts at offset 0 within our buffer (since we read from sector 2)
        // Actually wait - we need to read from offset 1024 which is sector 2 for 512-byte sectors
        // But our buffer starts at sector 2, so superblock is at the start
        let sb: Ext4Superblock =
            unsafe { core::ptr::read_unaligned(buf.as_ptr() as *const Ext4Superblock) };

        // Validate magic
        if sb.s_magic != EXT4_MAGIC {
            return Err(Ext4Error::InvalidMagic);
        }

        Ok(sb)
    }

    /// Get block size in bytes
    pub fn block_size(&self) -> u64 {
        1024u64 << self.s_log_block_size
    }

    /// Get total block count (64-bit)
    pub fn blocks_count(&self) -> u64 {
        self.s_blocks_count_lo as u64 | ((self.s_blocks_count_hi as u64) << 32)
    }

    /// Get free blocks count (64-bit)
    pub fn free_blocks_count(&self) -> u64 {
        self.s_free_blocks_count_lo as u64 | ((self.s_free_blocks_count_hi as u64) << 32)
    }

    /// Get inode size
    pub fn inode_size(&self) -> u32 {
        if self.s_rev_level >= 1 && self.s_inode_size > 0 {
            self.s_inode_size as u32
        } else {
            128 // Default for old revision
        }
    }

    /// Get block group descriptor size
    pub fn desc_size(&self) -> u32 {
        if self.s_feature_incompat & incompat::_64BIT != 0 && self.s_desc_size > 0 {
            self.s_desc_size as u32
        } else {
            32 // Classic 32-byte descriptor
        }
    }

    /// Check if 64-bit mode is enabled
    pub fn is_64bit(&self) -> bool {
        self.s_feature_incompat & incompat::_64BIT != 0
    }

    /// Check if extents are enabled
    pub fn has_extents(&self) -> bool {
        self.s_feature_incompat & incompat::EXTENTS != 0
    }

    /// Check if filesystem has a journal
    pub fn has_journal(&self) -> bool {
        self.s_feature_compat & compat::HAS_JOURNAL != 0
    }

    /// Calculate number of block groups
    pub fn block_group_count(&self) -> u32 {
        let blocks = self.blocks_count();
        let bpg = self.s_blocks_per_group as u64;
        ((blocks + bpg - 1) / bpg) as u32
    }

    /// Get the block containing the block group descriptors
    pub fn group_desc_block(&self) -> u64 {
        // Block group descriptors start in the block after the superblock
        // For block size 1024, superblock is in block 1, so descriptors start at block 2
        // For larger blocks, superblock is in block 0, descriptors start at block 1
        if self.block_size() == 1024 { 2 } else { 1 }
    }

    /// Validate superblock for required features
    pub fn validate(&self) -> Ext4Result<()> {
        // Check for incompatible features we don't support
        let unsupported_incompat = incompat::COMPRESSION
            | incompat::JOURNAL_DEV
            | incompat::MMP
            | incompat::EA_INODE
            | incompat::INLINE_DATA
            | incompat::ENCRYPT;

        if self.s_feature_incompat & unsupported_incompat != 0 {
            return Err(Ext4Error::UnsupportedFeature);
        }

        Ok(())
    }
}
