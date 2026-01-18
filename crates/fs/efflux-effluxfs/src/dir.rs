//! EFFLUXFS Directory handling

use alloc::string::String;
use alloc::vec::Vec;

use crate::{EffluxfsError, EffluxfsResult, MAX_NAME_LEN};

/// Directory entry (on-disk format)
#[repr(C)]
#[derive(Debug, Clone)]
pub struct DirEntry {
    /// Inode number
    pub ino: u64,
    /// Entry length (for variable-length names)
    pub rec_len: u16,
    /// Name length
    pub name_len: u8,
    /// File type (for fast readdir)
    pub file_type: u8,
    /// Name (variable length, null-terminated)
    pub name: [u8; MAX_NAME_LEN + 1],
}

impl DirEntry {
    /// Create a new directory entry
    pub fn new(ino: u64, name: &str, file_type: u8) -> EffluxfsResult<Self> {
        if name.len() > MAX_NAME_LEN {
            return Err(EffluxfsError::NameTooLong);
        }

        let mut entry = DirEntry {
            ino,
            rec_len: 0,
            name_len: name.len() as u8,
            file_type,
            name: [0u8; MAX_NAME_LEN + 1],
        };

        entry.name[..name.len()].copy_from_slice(name.as_bytes());

        // Calculate record length (8 + 2 + 1 + 1 + name_len, rounded up to 8)
        entry.rec_len = ((12 + name.len() + 7) / 8 * 8) as u16;

        Ok(entry)
    }

    /// Parse from bytes
    pub fn parse(data: &[u8]) -> EffluxfsResult<Self> {
        if data.len() < 12 {
            return Err(EffluxfsError::CorruptedInode);
        }

        let ino = u64::from_le_bytes([
            data[0], data[1], data[2], data[3],
            data[4], data[5], data[6], data[7],
        ]);

        let rec_len = u16::from_le_bytes([data[8], data[9]]);
        let name_len = data[10];
        let file_type = data[11];

        if data.len() < 12 + name_len as usize {
            return Err(EffluxfsError::CorruptedInode);
        }

        let mut name = [0u8; MAX_NAME_LEN + 1];
        name[..name_len as usize].copy_from_slice(&data[12..12 + name_len as usize]);

        Ok(DirEntry {
            ino,
            rec_len,
            name_len,
            file_type,
            name,
        })
    }

    /// Serialize to bytes
    pub fn serialize(&self, buf: &mut [u8]) {
        buf[0..8].copy_from_slice(&self.ino.to_le_bytes());
        buf[8..10].copy_from_slice(&self.rec_len.to_le_bytes());
        buf[10] = self.name_len;
        buf[11] = self.file_type;
        buf[12..12 + self.name_len as usize]
            .copy_from_slice(&self.name[..self.name_len as usize]);
    }

    /// Get name as string
    pub fn name_str(&self) -> &str {
        unsafe {
            core::str::from_utf8_unchecked(&self.name[..self.name_len as usize])
        }
    }

    /// Check if this is a deleted entry
    pub fn is_deleted(&self) -> bool {
        self.ino == 0
    }
}

/// File type constants for directory entries
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

/// Initialize a new directory with . and .. entries
pub fn init_directory(buf: &mut [u8], self_ino: u64, parent_ino: u64) {
    // Create . entry
    let dot = DirEntry::new(self_ino, ".", file_type::DIR).unwrap();
    dot.serialize(&mut buf[0..]);

    // Create .. entry
    let dotdot = DirEntry::new(parent_ino, "..", file_type::DIR).unwrap();
    dotdot.serialize(&mut buf[dot.rec_len as usize..]);

    // Set rec_len of .. to fill rest of block
    let remaining = buf.len() - dot.rec_len as usize;
    buf[dot.rec_len as usize + 8] = (remaining & 0xFF) as u8;
    buf[dot.rec_len as usize + 9] = ((remaining >> 8) & 0xFF) as u8;
}

/// Find an entry in a directory block
pub fn find_entry(buf: &[u8], name: &str) -> Option<DirEntry> {
    let mut offset = 0;

    while offset < buf.len() {
        let entry = DirEntry::parse(&buf[offset..]).ok()?;

        if entry.rec_len == 0 {
            break;
        }

        if !entry.is_deleted() && entry.name_str() == name {
            return Some(entry);
        }

        offset += entry.rec_len as usize;
    }

    None
}

/// Iterate over directory entries
pub fn iter_entries(buf: &[u8]) -> DirIterator {
    DirIterator { buf, offset: 0 }
}

/// Directory entry iterator
pub struct DirIterator<'a> {
    buf: &'a [u8],
    offset: usize,
}

impl<'a> Iterator for DirIterator<'a> {
    type Item = DirEntry;

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset >= self.buf.len() {
            return None;
        }

        let entry = DirEntry::parse(&self.buf[self.offset..]).ok()?;

        if entry.rec_len == 0 {
            return None;
        }

        self.offset += entry.rec_len as usize;

        if entry.is_deleted() {
            self.next()
        } else {
            Some(entry)
        }
    }
}

/// Add an entry to a directory block
///
/// Returns true if the entry was added, false if no space.
pub fn add_entry(buf: &mut [u8], ino: u64, name: &str, ftype: u8) -> EffluxfsResult<bool> {
    let new_entry = DirEntry::new(ino, name, ftype)?;
    let needed_len = new_entry.rec_len as usize;

    let mut offset = 0;
    while offset < buf.len() {
        let entry = DirEntry::parse(&buf[offset..])?;

        if entry.rec_len == 0 {
            break;
        }

        // Calculate actual size of current entry
        let actual_size = ((12 + entry.name_len as usize + 7) / 8 * 8) as usize;
        let free_space = entry.rec_len as usize - actual_size;

        if free_space >= needed_len {
            // Shrink current entry
            buf[offset + 8] = (actual_size & 0xFF) as u8;
            buf[offset + 9] = ((actual_size >> 8) & 0xFF) as u8;

            // Add new entry
            let new_offset = offset + actual_size;
            let mut new_entry = new_entry;
            new_entry.rec_len = (entry.rec_len as usize - actual_size) as u16;
            new_entry.serialize(&mut buf[new_offset..]);

            return Ok(true);
        }

        offset += entry.rec_len as usize;
    }

    Ok(false)
}

/// Remove an entry from a directory block
pub fn remove_entry(buf: &mut [u8], name: &str) -> EffluxfsResult<bool> {
    let mut offset = 0;
    let mut prev_offset: Option<usize> = None;

    while offset < buf.len() {
        let entry = DirEntry::parse(&buf[offset..])?;

        if entry.rec_len == 0 {
            break;
        }

        if !entry.is_deleted() && entry.name_str() == name {
            if let Some(prev) = prev_offset {
                // Merge with previous entry
                let prev_rec_len = u16::from_le_bytes([buf[prev + 8], buf[prev + 9]]);
                let new_rec_len = prev_rec_len + entry.rec_len;
                buf[prev + 8] = (new_rec_len & 0xFF) as u8;
                buf[prev + 9] = ((new_rec_len >> 8) & 0xFF) as u8;
            } else {
                // Mark as deleted
                buf[offset..offset + 8].copy_from_slice(&0u64.to_le_bytes());
            }
            return Ok(true);
        }

        prev_offset = Some(offset);
        offset += entry.rec_len as usize;
    }

    Ok(false)
}
