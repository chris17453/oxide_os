//! FAT32 BIOS Parameter Block (BPB)

use vfs::{VfsError, VfsResult};

/// BIOS Parameter Block
#[derive(Debug, Clone)]
pub struct Bpb {
    /// Bytes per sector (usually 512)
    pub bytes_per_sector: u16,
    /// Sectors per cluster
    pub sectors_per_cluster: u8,
    /// Reserved sectors (before FAT)
    pub reserved_sectors: u16,
    /// Number of FATs (usually 2)
    pub num_fats: u8,
    /// Root directory entry count (0 for FAT32)
    pub root_entry_count: u16,
    /// Total sectors (16-bit, 0 for FAT32)
    pub total_sectors_16: u16,
    /// Media type
    pub media_type: u8,
    /// FAT size in sectors (16-bit, 0 for FAT32)
    pub fat_size_16: u16,
    /// Sectors per track
    pub sectors_per_track: u16,
    /// Number of heads
    pub num_heads: u16,
    /// Hidden sectors
    pub hidden_sectors: u32,
    /// Total sectors (32-bit)
    pub total_sectors_32: u32,

    // FAT32-specific fields
    /// FAT size in sectors (32-bit)
    pub fat32_size: u32,
    /// Extended flags
    pub ext_flags: u16,
    /// Filesystem version
    pub fs_version: u16,
    /// Root directory first cluster
    pub root_cluster: u32,
    /// FSInfo sector number
    pub fs_info_sector: u16,
    /// Backup boot sector
    pub backup_boot_sector: u16,
    /// Volume serial number
    pub volume_serial: u32,
    /// Volume label
    pub volume_label: [u8; 11],
    /// Filesystem type string
    pub fs_type: [u8; 8],
}

impl Bpb {
    /// Parse BPB from boot sector
    pub fn parse(data: &[u8]) -> VfsResult<Self> {
        if data.len() < 512 {
            return Err(VfsError::InvalidFilesystem);
        }

        // Check boot signature
        if data[510] != 0x55 || data[511] != 0xAA {
            return Err(VfsError::InvalidFilesystem);
        }

        let bytes_per_sector = u16::from_le_bytes([data[11], data[12]]);
        let sectors_per_cluster = data[13];
        let reserved_sectors = u16::from_le_bytes([data[14], data[15]]);
        let num_fats = data[16];
        let root_entry_count = u16::from_le_bytes([data[17], data[18]]);
        let total_sectors_16 = u16::from_le_bytes([data[19], data[20]]);
        let media_type = data[21];
        let fat_size_16 = u16::from_le_bytes([data[22], data[23]]);
        let sectors_per_track = u16::from_le_bytes([data[24], data[25]]);
        let num_heads = u16::from_le_bytes([data[26], data[27]]);
        let hidden_sectors = u32::from_le_bytes([data[28], data[29], data[30], data[31]]);
        let total_sectors_32 = u32::from_le_bytes([data[32], data[33], data[34], data[35]]);

        // FAT32 extended BPB
        let fat32_size = u32::from_le_bytes([data[36], data[37], data[38], data[39]]);
        let ext_flags = u16::from_le_bytes([data[40], data[41]]);
        let fs_version = u16::from_le_bytes([data[42], data[43]]);
        let root_cluster = u32::from_le_bytes([data[44], data[45], data[46], data[47]]);
        let fs_info_sector = u16::from_le_bytes([data[48], data[49]]);
        let backup_boot_sector = u16::from_le_bytes([data[50], data[51]]);
        let volume_serial = u32::from_le_bytes([data[67], data[68], data[69], data[70]]);

        let mut volume_label = [0u8; 11];
        volume_label.copy_from_slice(&data[71..82]);

        let mut fs_type = [0u8; 8];
        fs_type.copy_from_slice(&data[82..90]);

        // Validate basic parameters
        if bytes_per_sector == 0 || !bytes_per_sector.is_power_of_two() {
            return Err(VfsError::InvalidFilesystem);
        }

        if sectors_per_cluster == 0 || !sectors_per_cluster.is_power_of_two() {
            return Err(VfsError::InvalidFilesystem);
        }

        Ok(Bpb {
            bytes_per_sector,
            sectors_per_cluster,
            reserved_sectors,
            num_fats,
            root_entry_count,
            total_sectors_16,
            media_type,
            fat_size_16,
            sectors_per_track,
            num_heads,
            hidden_sectors,
            total_sectors_32,
            fat32_size,
            ext_flags,
            fs_version,
            root_cluster,
            fs_info_sector,
            backup_boot_sector,
            volume_serial,
            volume_label,
            fs_type,
        })
    }

    /// Check if this is FAT32
    pub fn is_fat32(&self) -> bool {
        // FAT32 has fat_size_16 == 0 and uses fat32_size instead
        self.fat_size_16 == 0 && self.fat32_size != 0
    }

    /// Get total sectors
    pub fn total_sectors(&self) -> u64 {
        if self.total_sectors_16 != 0 {
            self.total_sectors_16 as u64
        } else {
            self.total_sectors_32 as u64
        }
    }

    /// Get FAT size in sectors
    pub fn fat_size(&self) -> u32 {
        if self.fat_size_16 != 0 {
            self.fat_size_16 as u32
        } else {
            self.fat32_size
        }
    }

    /// Get first FAT sector
    pub fn first_fat_sector(&self) -> u64 {
        self.reserved_sectors as u64
    }

    /// Get volume label as string
    pub fn volume_label_str(&self) -> &str {
        let len = self.volume_label
            .iter()
            .rposition(|&c| c != b' ')
            .map(|i| i + 1)
            .unwrap_or(0);

        core::str::from_utf8(&self.volume_label[..len]).unwrap_or("")
    }
}

/// FSInfo structure
#[derive(Debug, Clone)]
pub struct FsInfo {
    /// Free cluster count (0xFFFFFFFF if unknown)
    pub free_count: u32,
    /// Next free cluster hint
    pub next_free: u32,
}

impl FsInfo {
    /// FSInfo signature 1
    const LEAD_SIG: u32 = 0x41615252;
    /// FSInfo signature 2
    const STRUC_SIG: u32 = 0x61417272;
    /// FSInfo signature 3
    const TRAIL_SIG: u32 = 0xAA550000;

    /// Parse FSInfo from sector
    pub fn parse(data: &[u8]) -> VfsResult<Self> {
        if data.len() < 512 {
            return Err(VfsError::InvalidFilesystem);
        }

        let lead_sig = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let struc_sig = u32::from_le_bytes([data[484], data[485], data[486], data[487]]);
        let trail_sig = u32::from_le_bytes([data[508], data[509], data[510], data[511]]);

        if lead_sig != Self::LEAD_SIG || struc_sig != Self::STRUC_SIG || trail_sig != Self::TRAIL_SIG {
            return Err(VfsError::InvalidFilesystem);
        }

        let free_count = u32::from_le_bytes([data[488], data[489], data[490], data[491]]);
        let next_free = u32::from_le_bytes([data[492], data[493], data[494], data[495]]);

        Ok(FsInfo {
            free_count,
            next_free,
        })
    }

    /// Serialize FSInfo to buffer
    pub fn serialize(&self, buf: &mut [u8]) {
        buf[0..4].copy_from_slice(&Self::LEAD_SIG.to_le_bytes());
        buf[484..488].copy_from_slice(&Self::STRUC_SIG.to_le_bytes());
        buf[488..492].copy_from_slice(&self.free_count.to_le_bytes());
        buf[492..496].copy_from_slice(&self.next_free.to_le_bytes());
        buf[508..512].copy_from_slice(&Self::TRAIL_SIG.to_le_bytes());
    }
}
