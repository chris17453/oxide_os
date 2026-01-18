//! GPT Partition Table Parser for EFFLUX OS
//!
//! Provides GPT (GUID Partition Table) parsing and partition enumeration.

#![no_std]

extern crate alloc;

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use efflux_block::{BlockDevice, BlockError, BlockResult, Partition};

/// GPT signature
const GPT_SIGNATURE: u64 = 0x5452415020494645; // "EFI PART" in little-endian

/// Protective MBR signature
const MBR_SIGNATURE: u16 = 0xAA55;

/// GPT header size
const GPT_HEADER_SIZE: usize = 92;

/// GPT partition entry size (minimum)
const GPT_ENTRY_SIZE: usize = 128;

/// GPT error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GptError {
    /// No GPT signature found
    NoSignature,
    /// Invalid header
    InvalidHeader,
    /// CRC mismatch
    CrcMismatch,
    /// Invalid partition entry
    InvalidEntry,
    /// I/O error
    IoError,
    /// Not a GPT disk
    NotGpt,
}

impl From<BlockError> for GptError {
    fn from(_: BlockError) -> Self {
        GptError::IoError
    }
}

/// GPT partition type GUIDs (common ones)
pub mod partition_types {
    /// EFI System Partition
    pub const EFI_SYSTEM: [u8; 16] = [
        0x28, 0x73, 0x2a, 0xc1, 0x1f, 0xf8, 0xd2, 0x11,
        0xba, 0x4b, 0x00, 0xa0, 0xc9, 0x3e, 0xc9, 0x3b,
    ];

    /// Microsoft Basic Data
    pub const MS_BASIC_DATA: [u8; 16] = [
        0xa2, 0xa0, 0xd0, 0xeb, 0xe5, 0xb9, 0x33, 0x44,
        0x87, 0xc0, 0x68, 0xb6, 0xb7, 0x26, 0x99, 0xc7,
    ];

    /// Linux filesystem
    pub const LINUX_FS: [u8; 16] = [
        0xaf, 0x3d, 0xc6, 0x0f, 0x83, 0x84, 0x72, 0x47,
        0x8e, 0x79, 0x3d, 0x69, 0xd8, 0x47, 0x7d, 0xe4,
    ];

    /// Linux swap
    pub const LINUX_SWAP: [u8; 16] = [
        0x6d, 0xfd, 0x57, 0x06, 0xab, 0xa4, 0xc4, 0x43,
        0x84, 0xe5, 0x09, 0x33, 0xc8, 0x4b, 0x4f, 0x4f,
    ];

    /// EFFLUX filesystem
    pub const EFFLUX_FS: [u8; 16] = [
        0x45, 0x46, 0x46, 0x4c, 0x55, 0x58, 0x46, 0x53,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
    ];
}

/// GPT header structure
#[derive(Debug, Clone)]
pub struct GptHeader {
    /// Signature ("EFI PART")
    pub signature: u64,
    /// Revision (usually 0x00010000)
    pub revision: u32,
    /// Header size
    pub header_size: u32,
    /// CRC32 of header
    pub header_crc32: u32,
    /// Reserved (must be 0)
    pub reserved: u32,
    /// LBA of this header
    pub my_lba: u64,
    /// LBA of alternate header
    pub alternate_lba: u64,
    /// First usable LBA
    pub first_usable_lba: u64,
    /// Last usable LBA
    pub last_usable_lba: u64,
    /// Disk GUID
    pub disk_guid: [u8; 16],
    /// Starting LBA of partition entries
    pub partition_entry_lba: u64,
    /// Number of partition entries
    pub num_partition_entries: u32,
    /// Size of each partition entry
    pub partition_entry_size: u32,
    /// CRC32 of partition entries
    pub partition_entries_crc32: u32,
}

impl GptHeader {
    /// Parse a GPT header from bytes
    pub fn parse(data: &[u8]) -> Result<Self, GptError> {
        if data.len() < GPT_HEADER_SIZE {
            return Err(GptError::InvalidHeader);
        }

        let signature = u64::from_le_bytes([
            data[0], data[1], data[2], data[3],
            data[4], data[5], data[6], data[7],
        ]);

        if signature != GPT_SIGNATURE {
            return Err(GptError::NoSignature);
        }

        Ok(GptHeader {
            signature,
            revision: u32::from_le_bytes([data[8], data[9], data[10], data[11]]),
            header_size: u32::from_le_bytes([data[12], data[13], data[14], data[15]]),
            header_crc32: u32::from_le_bytes([data[16], data[17], data[18], data[19]]),
            reserved: u32::from_le_bytes([data[20], data[21], data[22], data[23]]),
            my_lba: u64::from_le_bytes([
                data[24], data[25], data[26], data[27],
                data[28], data[29], data[30], data[31],
            ]),
            alternate_lba: u64::from_le_bytes([
                data[32], data[33], data[34], data[35],
                data[36], data[37], data[38], data[39],
            ]),
            first_usable_lba: u64::from_le_bytes([
                data[40], data[41], data[42], data[43],
                data[44], data[45], data[46], data[47],
            ]),
            last_usable_lba: u64::from_le_bytes([
                data[48], data[49], data[50], data[51],
                data[52], data[53], data[54], data[55],
            ]),
            disk_guid: [
                data[56], data[57], data[58], data[59],
                data[60], data[61], data[62], data[63],
                data[64], data[65], data[66], data[67],
                data[68], data[69], data[70], data[71],
            ],
            partition_entry_lba: u64::from_le_bytes([
                data[72], data[73], data[74], data[75],
                data[76], data[77], data[78], data[79],
            ]),
            num_partition_entries: u32::from_le_bytes([data[80], data[81], data[82], data[83]]),
            partition_entry_size: u32::from_le_bytes([data[84], data[85], data[86], data[87]]),
            partition_entries_crc32: u32::from_le_bytes([data[88], data[89], data[90], data[91]]),
        })
    }
}

/// GPT partition entry
#[derive(Debug, Clone)]
pub struct GptEntry {
    /// Partition type GUID
    pub type_guid: [u8; 16],
    /// Partition unique GUID
    pub partition_guid: [u8; 16],
    /// First LBA
    pub first_lba: u64,
    /// Last LBA (inclusive)
    pub last_lba: u64,
    /// Attribute flags
    pub attributes: u64,
    /// Partition name (UTF-16LE, up to 36 code units)
    pub name: [u16; 36],
}

impl GptEntry {
    /// Parse a GPT entry from bytes
    pub fn parse(data: &[u8]) -> Result<Self, GptError> {
        if data.len() < GPT_ENTRY_SIZE {
            return Err(GptError::InvalidEntry);
        }

        let mut type_guid = [0u8; 16];
        type_guid.copy_from_slice(&data[0..16]);

        let mut partition_guid = [0u8; 16];
        partition_guid.copy_from_slice(&data[16..32]);

        let first_lba = u64::from_le_bytes([
            data[32], data[33], data[34], data[35],
            data[36], data[37], data[38], data[39],
        ]);

        let last_lba = u64::from_le_bytes([
            data[40], data[41], data[42], data[43],
            data[44], data[45], data[46], data[47],
        ]);

        let attributes = u64::from_le_bytes([
            data[48], data[49], data[50], data[51],
            data[52], data[53], data[54], data[55],
        ]);

        let mut name = [0u16; 36];
        for i in 0..36 {
            let offset = 56 + i * 2;
            name[i] = u16::from_le_bytes([data[offset], data[offset + 1]]);
        }

        Ok(GptEntry {
            type_guid,
            partition_guid,
            first_lba,
            last_lba,
            attributes,
            name,
        })
    }

    /// Check if entry is empty (unused)
    pub fn is_empty(&self) -> bool {
        self.type_guid == [0u8; 16]
    }

    /// Get partition size in blocks
    pub fn size_blocks(&self) -> u64 {
        if self.is_empty() {
            0
        } else {
            self.last_lba - self.first_lba + 1
        }
    }

    /// Get partition name as String
    pub fn name_string(&self) -> String {
        let mut s = String::new();
        for &c in &self.name {
            if c == 0 {
                break;
            }
            if let Some(ch) = char::from_u32(c as u32) {
                s.push(ch);
            }
        }
        s
    }

    /// Check if this is an EFI system partition
    pub fn is_efi_system(&self) -> bool {
        self.type_guid == partition_types::EFI_SYSTEM
    }

    /// Check if this is a Linux filesystem partition
    pub fn is_linux_fs(&self) -> bool {
        self.type_guid == partition_types::LINUX_FS
    }

    /// Check if this is an EFFLUX filesystem partition
    pub fn is_efflux_fs(&self) -> bool {
        self.type_guid == partition_types::EFFLUX_FS
    }
}

/// GPT disk representation
pub struct Gpt {
    /// Primary header
    pub header: GptHeader,
    /// Partition entries
    pub entries: Vec<GptEntry>,
}

impl Gpt {
    /// Parse GPT from a block device
    pub fn parse(device: &dyn BlockDevice) -> Result<Self, GptError> {
        let block_size = device.block_size() as usize;
        let mut buf = alloc::vec![0u8; block_size];

        // Read LBA 1 (GPT header)
        device.read(1, &mut buf)?;
        let header = GptHeader::parse(&buf)?;

        // Read partition entries
        let entries_per_block = block_size / header.partition_entry_size as usize;
        let blocks_for_entries = (header.num_partition_entries as usize + entries_per_block - 1)
            / entries_per_block;

        let mut entries = Vec::new();
        let mut entry_buf = alloc::vec![0u8; block_size];

        for block in 0..blocks_for_entries {
            let lba = header.partition_entry_lba + block as u64;
            device.read(lba, &mut entry_buf)?;

            for i in 0..entries_per_block {
                let entry_idx = block * entries_per_block + i;
                if entry_idx >= header.num_partition_entries as usize {
                    break;
                }

                let offset = i * header.partition_entry_size as usize;
                let entry = GptEntry::parse(&entry_buf[offset..])?;

                if !entry.is_empty() {
                    entries.push(entry);
                }
            }
        }

        Ok(Gpt { header, entries })
    }

    /// Get partitions as Partition objects
    pub fn partitions(&self, device: Arc<dyn BlockDevice>) -> Vec<Partition> {
        self.entries
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                Partition::new(
                    Arc::clone(&device),
                    entry.first_lba,
                    entry.size_blocks(),
                    (i + 1) as u8,
                    // TODO: Use entry.name_string() - need to make name &'static
                    "partition",
                )
            })
            .collect()
    }
}

/// Check if a block device has a GPT
pub fn has_gpt(device: &dyn BlockDevice) -> bool {
    let block_size = device.block_size() as usize;
    let mut buf = alloc::vec![0u8; block_size];

    // Try to read LBA 1 (GPT header)
    if device.read(1, &mut buf).is_err() {
        return false;
    }

    // Check signature
    if buf.len() < 8 {
        return false;
    }

    let signature = u64::from_le_bytes([
        buf[0], buf[1], buf[2], buf[3],
        buf[4], buf[5], buf[6], buf[7],
    ]);

    signature == GPT_SIGNATURE
}
