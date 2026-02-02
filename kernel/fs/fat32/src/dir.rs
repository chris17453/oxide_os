//! FAT32 Directory Entry structures

use alloc::string::String;
use alloc::vec::Vec;

/// FAT32 directory entry (32 bytes)
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct DirEntry {
    /// Short filename (8.3 format)
    pub name: [u8; 8],
    /// Extension
    pub ext: [u8; 3],
    /// Attributes
    pub attr: u8,
    /// Reserved (NT)
    pub nt_reserved: u8,
    /// Creation time tenths of second
    pub create_time_tenth: u8,
    /// Creation time
    pub create_time: u16,
    /// Creation date
    pub create_date: u16,
    /// Last access date
    pub access_date: u16,
    /// First cluster high 16 bits
    pub first_cluster_hi: u16,
    /// Modification time
    pub modify_time: u16,
    /// Modification date
    pub modify_date: u16,
    /// First cluster low 16 bits
    pub first_cluster_lo: u16,
    /// File size
    pub size: u32,
}

/// Directory entry attributes
pub mod attr {
    pub const READ_ONLY: u8 = 0x01;
    pub const HIDDEN: u8 = 0x02;
    pub const SYSTEM: u8 = 0x04;
    pub const VOLUME_ID: u8 = 0x08;
    pub const DIRECTORY: u8 = 0x10;
    pub const ARCHIVE: u8 = 0x20;
    pub const LONG_NAME: u8 = 0x0F;
}

impl DirEntry {
    /// Parse directory entry from bytes
    pub fn parse(data: &[u8]) -> Self {
        let mut name = [0u8; 8];
        let mut ext = [0u8; 3];
        name.copy_from_slice(&data[0..8]);
        ext.copy_from_slice(&data[8..11]);

        DirEntry {
            name,
            ext,
            attr: data[11],
            nt_reserved: data[12],
            create_time_tenth: data[13],
            create_time: u16::from_le_bytes([data[14], data[15]]),
            create_date: u16::from_le_bytes([data[16], data[17]]),
            access_date: u16::from_le_bytes([data[18], data[19]]),
            first_cluster_hi: u16::from_le_bytes([data[20], data[21]]),
            modify_time: u16::from_le_bytes([data[22], data[23]]),
            modify_date: u16::from_le_bytes([data[24], data[25]]),
            first_cluster_lo: u16::from_le_bytes([data[26], data[27]]),
            size: u32::from_le_bytes([data[28], data[29], data[30], data[31]]),
        }
    }

    /// Serialize to bytes
    pub fn serialize(&self, buf: &mut [u8]) {
        buf[0..8].copy_from_slice(&self.name);
        buf[8..11].copy_from_slice(&self.ext);
        buf[11] = self.attr;
        buf[12] = self.nt_reserved;
        buf[13] = self.create_time_tenth;
        buf[14..16].copy_from_slice(&self.create_time.to_le_bytes());
        buf[16..18].copy_from_slice(&self.create_date.to_le_bytes());
        buf[18..20].copy_from_slice(&self.access_date.to_le_bytes());
        buf[20..22].copy_from_slice(&self.first_cluster_hi.to_le_bytes());
        buf[22..24].copy_from_slice(&self.modify_time.to_le_bytes());
        buf[24..26].copy_from_slice(&self.modify_date.to_le_bytes());
        buf[26..28].copy_from_slice(&self.first_cluster_lo.to_le_bytes());
        buf[28..32].copy_from_slice(&self.size.to_le_bytes());
    }

    /// Get 8.3 name as string
    pub fn name_83(&self) -> String {
        let name = core::str::from_utf8(&self.name).unwrap_or("").trim_end();

        let ext = core::str::from_utf8(&self.ext).unwrap_or("").trim_end();

        if ext.is_empty() {
            String::from(name)
        } else {
            let mut result = String::from(name);
            result.push('.');
            result.push_str(ext);
            result
        }
    }

    /// Check if this is a directory
    pub fn is_directory(&self) -> bool {
        (self.attr & attr::DIRECTORY) != 0
    }

    /// Check if this is a volume label
    pub fn is_volume_label(&self) -> bool {
        (self.attr & attr::VOLUME_ID) != 0 && (self.attr & attr::DIRECTORY) == 0
    }

    /// Check if this is a long filename entry
    pub fn is_lfn(&self) -> bool {
        self.attr == attr::LONG_NAME
    }

    /// Check if entry is deleted
    pub fn is_deleted(&self) -> bool {
        self.name[0] == 0xE5
    }

    /// Check if entry is end of directory
    pub fn is_end(&self) -> bool {
        self.name[0] == 0x00
    }

    /// Get first cluster
    pub fn first_cluster(&self) -> u32 {
        ((self.first_cluster_hi as u32) << 16) | (self.first_cluster_lo as u32)
    }

    /// Create a short name entry
    pub fn new_short(name: &str, ext: &str, attr: u8, cluster: u32, size: u32) -> Self {
        let mut entry_name = [b' '; 8];
        let mut entry_ext = [b' '; 3];

        let name_bytes = name.as_bytes();
        let ext_bytes = ext.as_bytes();

        for (i, &b) in name_bytes.iter().take(8).enumerate() {
            entry_name[i] = b.to_ascii_uppercase();
        }

        for (i, &b) in ext_bytes.iter().take(3).enumerate() {
            entry_ext[i] = b.to_ascii_uppercase();
        }

        DirEntry {
            name: entry_name,
            ext: entry_ext,
            attr,
            nt_reserved: 0,
            create_time_tenth: 0,
            create_time: 0,
            create_date: 0,
            access_date: 0,
            first_cluster_hi: (cluster >> 16) as u16,
            modify_time: 0,
            modify_date: 0,
            first_cluster_lo: cluster as u16,
            size,
        }
    }
}

/// Long Filename Entry (32 bytes)
#[derive(Debug, Clone)]
pub struct LfnEntry {
    /// Sequence number
    pub seq: u8,
    /// Characters 1-5 (UCS-2)
    pub name1: [u16; 5],
    /// Attribute (always 0x0F)
    pub attr: u8,
    /// Type (always 0)
    pub entry_type: u8,
    /// Checksum of short name
    pub checksum: u8,
    /// Characters 6-11 (UCS-2)
    pub name2: [u16; 6],
    /// First cluster (always 0)
    pub first_cluster: u16,
    /// Characters 12-13 (UCS-2)
    pub name3: [u16; 2],
}

impl LfnEntry {
    /// Last LFN entry marker
    pub const LAST_LFN_ENTRY: u8 = 0x40;

    /// Parse LFN entry from bytes
    pub fn parse(data: &[u8]) -> Self {
        let mut name1 = [0u16; 5];
        let mut name2 = [0u16; 6];
        let mut name3 = [0u16; 2];

        for i in 0..5 {
            name1[i] = u16::from_le_bytes([data[1 + i * 2], data[2 + i * 2]]);
        }

        for i in 0..6 {
            name2[i] = u16::from_le_bytes([data[14 + i * 2], data[15 + i * 2]]);
        }

        for i in 0..2 {
            name3[i] = u16::from_le_bytes([data[28 + i * 2], data[29 + i * 2]]);
        }

        LfnEntry {
            seq: data[0],
            name1,
            attr: data[11],
            entry_type: data[12],
            checksum: data[13],
            name2,
            first_cluster: u16::from_le_bytes([data[26], data[27]]),
            name3,
        }
    }

    /// Serialize to bytes
    pub fn serialize(&self, buf: &mut [u8]) {
        buf[0] = self.seq;

        for i in 0..5 {
            let bytes = self.name1[i].to_le_bytes();
            buf[1 + i * 2] = bytes[0];
            buf[2 + i * 2] = bytes[1];
        }

        buf[11] = self.attr;
        buf[12] = self.entry_type;
        buf[13] = self.checksum;

        for i in 0..6 {
            let bytes = self.name2[i].to_le_bytes();
            buf[14 + i * 2] = bytes[0];
            buf[15 + i * 2] = bytes[1];
        }

        buf[26..28].copy_from_slice(&self.first_cluster.to_le_bytes());

        for i in 0..2 {
            let bytes = self.name3[i].to_le_bytes();
            buf[28 + i * 2] = bytes[0];
            buf[29 + i * 2] = bytes[1];
        }
    }

    /// Get sequence number (without LAST flag)
    pub fn sequence(&self) -> u8 {
        self.seq & 0x1F
    }

    /// Check if this is the last LFN entry
    pub fn is_last(&self) -> bool {
        (self.seq & Self::LAST_LFN_ENTRY) != 0
    }

    /// Get characters from this entry
    pub fn chars(&self) -> Vec<u16> {
        let mut chars = Vec::new();

        for &c in &self.name1 {
            if c == 0x0000 || c == 0xFFFF {
                return chars;
            }
            chars.push(c);
        }

        for &c in &self.name2 {
            if c == 0x0000 || c == 0xFFFF {
                return chars;
            }
            chars.push(c);
        }

        for &c in &self.name3 {
            if c == 0x0000 || c == 0xFFFF {
                return chars;
            }
            chars.push(c);
        }

        chars
    }

    /// Combine multiple LFN entries into a filename
    pub fn combine(entries: &[LfnEntry]) -> String {
        let mut chars: Vec<u16> = Vec::new();

        for entry in entries {
            chars.extend(entry.chars());
        }

        // Convert UCS-2 to UTF-8
        String::from_utf16_lossy(&chars)
    }

    /// Calculate checksum for short name
    pub fn checksum_83(name: &[u8; 8], ext: &[u8; 3]) -> u8 {
        let mut sum: u8 = 0;

        for &b in name.iter() {
            sum = sum.rotate_right(1).wrapping_add(b);
        }

        for &b in ext.iter() {
            sum = sum.rotate_right(1).wrapping_add(b);
        }

        sum
    }

    /// Create LFN entries for a long name
    pub fn create_entries(name: &str, checksum: u8) -> Vec<LfnEntry> {
        // Convert to UCS-2
        let chars: Vec<u16> = name.encode_utf16().collect();
        let num_entries = (chars.len() + 12) / 13; // 13 chars per entry

        let mut entries = Vec::with_capacity(num_entries);

        for i in 0..num_entries {
            let seq = if i == num_entries - 1 {
                (i + 1) as u8 | Self::LAST_LFN_ENTRY
            } else {
                (i + 1) as u8
            };

            let start = i * 13;
            let mut name1 = [0xFFFFu16; 5];
            let mut name2 = [0xFFFFu16; 6];
            let mut name3 = [0xFFFFu16; 2];

            for j in 0..5 {
                let idx = start + j;
                if idx < chars.len() {
                    name1[j] = chars[idx];
                } else if idx == chars.len() {
                    name1[j] = 0;
                }
            }

            for j in 0..6 {
                let idx = start + 5 + j;
                if idx < chars.len() {
                    name2[j] = chars[idx];
                } else if idx == chars.len() {
                    name2[j] = 0;
                }
            }

            for j in 0..2 {
                let idx = start + 11 + j;
                if idx < chars.len() {
                    name3[j] = chars[idx];
                } else if idx == chars.len() {
                    name3[j] = 0;
                }
            }

            entries.push(LfnEntry {
                seq,
                name1,
                attr: attr::LONG_NAME,
                entry_type: 0,
                checksum,
                name2,
                first_cluster: 0,
                name3,
            });
        }

        // Reverse so highest sequence is first
        entries.reverse();
        entries
    }
}

/// Convert FAT date/time to Unix timestamp
pub fn fat_to_unix_time(date: u16, time: u16) -> u64 {
    let year = ((date >> 9) & 0x7F) as u64 + 1980;
    let month = ((date >> 5) & 0x0F) as u64;
    let day = (date & 0x1F) as u64;

    let hour = ((time >> 11) & 0x1F) as u64;
    let minute = ((time >> 5) & 0x3F) as u64;
    let second = ((time & 0x1F) * 2) as u64;

    // Simplified calculation (doesn't handle all edge cases)
    let days_since_1970 = (year - 1970) * 365
        + (year - 1969) / 4
        + days_before_month(month, is_leap_year(year))
        + day
        - 1;

    days_since_1970 * 86400 + hour * 3600 + minute * 60 + second
}

/// Days before a month in a year
fn days_before_month(month: u64, leap: bool) -> u64 {
    const NORMAL: [u64; 13] = [0, 0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    const LEAP: [u64; 13] = [0, 0, 31, 60, 91, 121, 152, 182, 213, 244, 274, 305, 335];

    if leap {
        LEAP[month as usize]
    } else {
        NORMAL[month as usize]
    }
}

/// Check if a year is a leap year
fn is_leap_year(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}
