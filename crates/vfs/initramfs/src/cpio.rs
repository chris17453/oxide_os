//! CPIO archive parser (newc format)
//!
//! Parses cpio archives in the "newc" format used by Linux initramfs.

use alloc::string::String;
use alloc::vec::Vec;

/// CPIO newc header size
const HEADER_SIZE: usize = 110;

/// CPIO newc magic
const MAGIC: &[u8] = b"070701";

/// Trailer filename marking end of archive
const TRAILER: &str = "TRAILER!!!";

/// Error during CPIO parsing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpioError {
    /// Invalid magic number
    InvalidMagic,
    /// Invalid header format
    InvalidHeader,
    /// Unexpected end of data
    UnexpectedEof,
    /// Invalid UTF-8 in filename
    InvalidFilename,
}

/// A file entry from a CPIO archive
#[derive(Debug, Clone)]
pub struct CpioEntry {
    /// Filename (full path)
    pub name: String,
    /// File mode (permissions + type)
    pub mode: u32,
    /// User ID
    pub uid: u32,
    /// Group ID
    pub gid: u32,
    /// Number of links
    pub nlink: u32,
    /// Modification time
    pub mtime: u32,
    /// Device major number (for device files)
    pub rdev_major: u32,
    /// Device minor number (for device files)
    pub rdev_minor: u32,
    /// File data
    pub data: Vec<u8>,
}

impl CpioEntry {
    /// Is this a regular file?
    pub fn is_file(&self) -> bool {
        (self.mode & 0o170000) == 0o100000
    }

    /// Is this a directory?
    pub fn is_dir(&self) -> bool {
        (self.mode & 0o170000) == 0o040000
    }

    /// Is this a symlink?
    pub fn is_symlink(&self) -> bool {
        (self.mode & 0o170000) == 0o120000
    }

    /// Is this a character device?
    pub fn is_char_device(&self) -> bool {
        (self.mode & 0o170000) == 0o020000
    }

    /// Is this a block device?
    pub fn is_block_device(&self) -> bool {
        (self.mode & 0o170000) == 0o060000
    }

    /// Is this a FIFO (named pipe)?
    pub fn is_fifo(&self) -> bool {
        (self.mode & 0o170000) == 0o010000
    }

    /// Is this a socket?
    pub fn is_socket(&self) -> bool {
        (self.mode & 0o170000) == 0o140000
    }

    /// Get permission bits only
    pub fn permissions(&self) -> u32 {
        self.mode & 0o7777
    }

    /// Get symlink target (data interpreted as UTF-8 string)
    pub fn symlink_target(&self) -> Option<&str> {
        if self.is_symlink() {
            core::str::from_utf8(&self.data).ok()
        } else {
            None
        }
    }

    /// Get device numbers (for char/block devices)
    /// Returns (major, minor)
    pub fn device_numbers(&self) -> (u32, u32) {
        (self.rdev_major, self.rdev_minor)
    }

    /// Get combined device number (major << 8 | minor)
    pub fn rdev(&self) -> u64 {
        ((self.rdev_major as u64) << 8) | (self.rdev_minor as u64 & 0xFF)
    }
}

/// Parse a hex string from ASCII bytes
fn parse_hex(bytes: &[u8]) -> Result<u32, CpioError> {
    let s = core::str::from_utf8(bytes).map_err(|_| CpioError::InvalidHeader)?;
    u32::from_str_radix(s, 16).map_err(|_| CpioError::InvalidHeader)
}

/// Align to 4-byte boundary
fn align4(n: usize) -> usize {
    (n + 3) & !3
}

/// CPIO archive iterator
pub struct CpioIterator<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> CpioIterator<'a> {
    /// Create a new CPIO iterator
    pub fn new(data: &'a [u8]) -> Self {
        CpioIterator { data, offset: 0 }
    }

    /// Parse the next entry
    fn parse_entry(&mut self) -> Result<Option<CpioEntry>, CpioError> {
        // Check for enough data for header
        if self.offset + HEADER_SIZE > self.data.len() {
            return Err(CpioError::UnexpectedEof);
        }

        let header = &self.data[self.offset..self.offset + HEADER_SIZE];

        // Verify magic
        if &header[0..6] != MAGIC {
            return Err(CpioError::InvalidMagic);
        }

        // Parse header fields
        // newc format:
        // 0-6: magic, 6-14: ino, 14-22: mode, 22-30: uid, 30-38: gid
        // 38-46: nlink, 46-54: mtime, 54-62: filesize, 62-70: devmajor
        // 70-78: devminor, 78-86: rdevmajor, 86-94: rdevminor, 94-102: namesize
        // 102-110: check
        let mode = parse_hex(&header[14..22])?;
        let uid = parse_hex(&header[22..30])?;
        let gid = parse_hex(&header[30..38])?;
        let nlink = parse_hex(&header[38..46])?;
        let mtime = parse_hex(&header[46..54])?;
        let filesize = parse_hex(&header[54..62])? as usize;
        let rdev_major = parse_hex(&header[78..86])?;
        let rdev_minor = parse_hex(&header[86..94])?;
        let namesize = parse_hex(&header[94..102])? as usize;

        // Move past header
        self.offset += HEADER_SIZE;

        // Read filename
        if self.offset + namesize > self.data.len() {
            return Err(CpioError::UnexpectedEof);
        }

        let name_bytes = &self.data[self.offset..self.offset + namesize];
        // Remove trailing NUL if present
        let name_end = name_bytes.iter().position(|&b| b == 0).unwrap_or(namesize);
        let name = core::str::from_utf8(&name_bytes[..name_end])
            .map_err(|_| CpioError::InvalidFilename)?
            .to_string();

        // Align past filename
        self.offset = align4(self.offset + namesize);

        // Check for trailer
        if name == TRAILER {
            return Ok(None);
        }

        // Read file data
        if self.offset + filesize > self.data.len() {
            return Err(CpioError::UnexpectedEof);
        }

        let data = self.data[self.offset..self.offset + filesize].to_vec();

        // Align past file data
        self.offset = align4(self.offset + filesize);

        Ok(Some(CpioEntry {
            name,
            mode,
            uid,
            gid,
            nlink,
            mtime,
            rdev_major,
            rdev_minor,
            data,
        }))
    }
}

impl<'a> Iterator for CpioIterator<'a> {
    type Item = Result<CpioEntry, CpioError>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.parse_entry() {
            Ok(Some(entry)) => Some(Ok(entry)),
            Ok(None) => None, // End of archive
            Err(e) => Some(Err(e)),
        }
    }
}

/// Parse all entries from a CPIO archive
pub fn parse(data: &[u8]) -> Result<Vec<CpioEntry>, CpioError> {
    CpioIterator::new(data).collect()
}

use alloc::string::ToString;
