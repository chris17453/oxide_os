//! OXIDEFS Superblock

use crate::{OXIDEFS_MAGIC, OxidefsError, OxidefsResult};

/// Superblock structure
#[derive(Debug, Clone)]
pub struct Superblock {
    /// Magic number
    pub magic: u64,
    /// Filesystem version
    pub version: u32,
    /// Block size
    pub block_size: u32,
    /// Total blocks
    pub total_blocks: u64,
    /// Free blocks
    pub free_blocks: u64,
    /// Total inodes
    pub total_inodes: u64,
    /// Free inodes
    pub free_inodes: u64,
    /// Block bitmap start block
    pub block_bitmap_start: u64,
    /// Inode bitmap start block
    pub inode_bitmap_start: u64,
    /// Inode table start block
    pub inode_table_start: u64,
    /// First data block
    pub first_data_block: u64,
    /// Root inode number
    pub root_inode: u64,
    /// Last mount time
    pub mount_time: u64,
    /// Last write time
    pub write_time: u64,
    /// Mount count since last fsck
    pub mount_count: u16,
    /// Maximum mount count before fsck
    pub max_mount_count: u16,
    /// Filesystem state (1 = clean, 2 = errors)
    pub state: u16,
    /// Checksum
    pub checksum: u32,
}

impl Superblock {
    /// Parse superblock from bytes
    pub fn parse(data: &[u8]) -> OxidefsResult<Self> {
        if data.len() < 128 {
            return Err(OxidefsError::CorruptedSuperblock);
        }

        let magic = u64::from_le_bytes([
            data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
        ]);

        if magic != OXIDEFS_MAGIC {
            return Err(OxidefsError::InvalidMagic);
        }

        Ok(Superblock {
            magic,
            version: u32::from_le_bytes([data[8], data[9], data[10], data[11]]),
            block_size: u32::from_le_bytes([data[12], data[13], data[14], data[15]]),
            total_blocks: u64::from_le_bytes([
                data[16], data[17], data[18], data[19], data[20], data[21], data[22], data[23],
            ]),
            free_blocks: u64::from_le_bytes([
                data[24], data[25], data[26], data[27], data[28], data[29], data[30], data[31],
            ]),
            total_inodes: u64::from_le_bytes([
                data[32], data[33], data[34], data[35], data[36], data[37], data[38], data[39],
            ]),
            free_inodes: u64::from_le_bytes([
                data[40], data[41], data[42], data[43], data[44], data[45], data[46], data[47],
            ]),
            block_bitmap_start: u64::from_le_bytes([
                data[48], data[49], data[50], data[51], data[52], data[53], data[54], data[55],
            ]),
            inode_bitmap_start: u64::from_le_bytes([
                data[56], data[57], data[58], data[59], data[60], data[61], data[62], data[63],
            ]),
            inode_table_start: u64::from_le_bytes([
                data[64], data[65], data[66], data[67], data[68], data[69], data[70], data[71],
            ]),
            first_data_block: u64::from_le_bytes([
                data[72], data[73], data[74], data[75], data[76], data[77], data[78], data[79],
            ]),
            root_inode: u64::from_le_bytes([
                data[80], data[81], data[82], data[83], data[84], data[85], data[86], data[87],
            ]),
            mount_time: u64::from_le_bytes([
                data[88], data[89], data[90], data[91], data[92], data[93], data[94], data[95],
            ]),
            write_time: u64::from_le_bytes([
                data[96], data[97], data[98], data[99], data[100], data[101], data[102], data[103],
            ]),
            mount_count: u16::from_le_bytes([data[104], data[105]]),
            max_mount_count: u16::from_le_bytes([data[106], data[107]]),
            state: u16::from_le_bytes([data[108], data[109]]),
            checksum: u32::from_le_bytes([data[110], data[111], data[112], data[113]]),
        })
    }

    /// Serialize superblock to bytes
    pub fn serialize(&self, buf: &mut [u8]) {
        buf[0..8].copy_from_slice(&self.magic.to_le_bytes());
        buf[8..12].copy_from_slice(&self.version.to_le_bytes());
        buf[12..16].copy_from_slice(&self.block_size.to_le_bytes());
        buf[16..24].copy_from_slice(&self.total_blocks.to_le_bytes());
        buf[24..32].copy_from_slice(&self.free_blocks.to_le_bytes());
        buf[32..40].copy_from_slice(&self.total_inodes.to_le_bytes());
        buf[40..48].copy_from_slice(&self.free_inodes.to_le_bytes());
        buf[48..56].copy_from_slice(&self.block_bitmap_start.to_le_bytes());
        buf[56..64].copy_from_slice(&self.inode_bitmap_start.to_le_bytes());
        buf[64..72].copy_from_slice(&self.inode_table_start.to_le_bytes());
        buf[72..80].copy_from_slice(&self.first_data_block.to_le_bytes());
        buf[80..88].copy_from_slice(&self.root_inode.to_le_bytes());
        buf[88..96].copy_from_slice(&self.mount_time.to_le_bytes());
        buf[96..104].copy_from_slice(&self.write_time.to_le_bytes());
        buf[104..106].copy_from_slice(&self.mount_count.to_le_bytes());
        buf[106..108].copy_from_slice(&self.max_mount_count.to_le_bytes());
        buf[108..110].copy_from_slice(&self.state.to_le_bytes());
        buf[110..114].copy_from_slice(&self.checksum.to_le_bytes());
    }

    /// Calculate checksum
    pub fn calculate_checksum(&self) -> u32 {
        // Simple CRC32-like checksum
        let mut sum: u32 = 0;
        sum = sum.wrapping_add((self.magic & 0xFFFFFFFF) as u32);
        sum = sum.wrapping_add((self.magic >> 32) as u32);
        sum = sum.wrapping_add(self.version);
        sum = sum.wrapping_add(self.block_size);
        sum = sum.wrapping_add((self.total_blocks & 0xFFFFFFFF) as u32);
        sum = sum.wrapping_add((self.total_blocks >> 32) as u32);
        sum = sum.wrapping_add((self.total_inodes & 0xFFFFFFFF) as u32);
        sum = sum.wrapping_add((self.total_inodes >> 32) as u32);
        sum
    }
}
