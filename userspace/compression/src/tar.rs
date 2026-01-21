//! TAR archive format support
//!
//! Implements POSIX ustar format (IEEE 1003.1-1988)

use crate::{CompressionError, Result};
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::str;

/// TAR block size (512 bytes)
pub const BLOCK_SIZE: usize = 512;

/// TAR file types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    /// Regular file
    Regular,
    /// Hard link
    Link,
    /// Symbolic link
    Symlink,
    /// Character device
    CharDevice,
    /// Block device
    BlockDevice,
    /// Directory
    Directory,
    /// FIFO
    Fifo,
}

impl FileType {
    /// Convert to TAR type flag
    pub fn to_flag(&self) -> u8 {
        match self {
            FileType::Regular => b'0',
            FileType::Link => b'1',
            FileType::Symlink => b'2',
            FileType::CharDevice => b'3',
            FileType::BlockDevice => b'4',
            FileType::Directory => b'5',
            FileType::Fifo => b'6',
        }
    }

    /// Convert from TAR type flag
    pub fn from_flag(flag: u8) -> Self {
        match flag {
            b'0' | 0 => FileType::Regular,
            b'1' => FileType::Link,
            b'2' => FileType::Symlink,
            b'3' => FileType::CharDevice,
            b'4' => FileType::BlockDevice,
            b'5' => FileType::Directory,
            b'6' => FileType::Fifo,
            _ => FileType::Regular, // Default
        }
    }
}

/// TAR header structure (POSIX ustar format)
#[repr(C)]
#[derive(Debug, Clone)]
pub struct TarHeader {
    /// File name (100 bytes)
    pub name: [u8; 100],
    /// File mode (8 bytes octal)
    pub mode: [u8; 8],
    /// Owner user ID (8 bytes octal)
    pub uid: [u8; 8],
    /// Owner group ID (8 bytes octal)
    pub gid: [u8; 8],
    /// File size (12 bytes octal)
    pub size: [u8; 12],
    /// Modification time (12 bytes octal)
    pub mtime: [u8; 12],
    /// Checksum (8 bytes octal)
    pub checksum: [u8; 8],
    /// Type flag
    pub typeflag: u8,
    /// Link name (100 bytes)
    pub linkname: [u8; 100],
    /// USTAR indicator ("ustar\0")
    pub magic: [u8; 6],
    /// USTAR version ("00")
    pub version: [u8; 2],
    /// Owner user name (32 bytes)
    pub uname: [u8; 32],
    /// Owner group name (32 bytes)
    pub gname: [u8; 32],
    /// Device major number (8 bytes octal)
    pub devmajor: [u8; 8],
    /// Device minor number (8 bytes octal)
    pub devminor: [u8; 8],
    /// Filename prefix (155 bytes)
    pub prefix: [u8; 155],
    /// Padding (12 bytes)
    pub padding: [u8; 12],
}

impl TarHeader {
    /// Create a new empty header
    pub fn new() -> Self {
        Self {
            name: [0; 100],
            mode: *b"0000644\0",
            uid: *b"0000000\0",
            gid: *b"0000000\0",
            size: [b'0'; 12],
            mtime: [b'0'; 12],
            checksum: [b' '; 8],
            typeflag: FileType::Regular.to_flag(),
            linkname: [0; 100],
            magic: *b"ustar\0",
            version: *b"00",
            uname: [0; 32],
            gname: [0; 32],
            devmajor: [b'0'; 8],
            devminor: [b'0'; 8],
            prefix: [0; 155],
            padding: [0; 12],
        }
    }

    /// Set the file name
    pub fn set_name(&mut self, name: &str) {
        let bytes = name.as_bytes();
        let len = bytes.len().min(100);
        self.name[..len].copy_from_slice(&bytes[..len]);
    }

    /// Get the file name
    pub fn get_name(&self) -> Result<String> {
        let end = self.name.iter().position(|&b| b == 0).unwrap_or(100);
        str::from_utf8(&self.name[..end])
            .map(|s| s.to_string())
            .map_err(|_| CompressionError::InvalidData)
    }

    /// Set the file size
    pub fn set_size(&mut self, size: u64) {
        write_octal(&mut self.size, size);
    }

    /// Get the file size
    pub fn get_size(&self) -> Result<u64> {
        parse_octal(&self.size)
    }

    /// Set the modification time
    pub fn set_mtime(&mut self, mtime: u64) {
        write_octal(&mut self.mtime, mtime);
    }

    /// Get the modification time
    pub fn get_mtime(&self) -> Result<u64> {
        parse_octal(&self.mtime)
    }

    /// Calculate and set the checksum
    pub fn update_checksum(&mut self) {
        // Checksum is calculated with checksum field set to spaces
        self.checksum = [b' '; 8];

        let bytes = unsafe {
            core::slice::from_raw_parts(self as *const Self as *const u8, BLOCK_SIZE)
        };

        let sum: u32 = bytes.iter().map(|&b| b as u32).sum();
        write_octal(&mut self.checksum[..7], sum as u64);
        self.checksum[7] = 0;
    }

    /// Verify the checksum
    pub fn verify_checksum(&self) -> bool {
        let mut temp = self.clone();
        temp.checksum = [b' '; 8];

        let bytes = unsafe {
            core::slice::from_raw_parts(&temp as *const Self as *const u8, BLOCK_SIZE)
        };

        let sum: u32 = bytes.iter().map(|&b| b as u32).sum();
        let stored_sum = parse_octal(&self.checksum[..7]).unwrap_or(0);

        sum as u64 == stored_sum
    }

    /// Convert to bytes
    pub fn to_bytes(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self as *const Self as *const u8, BLOCK_SIZE) }
    }

    /// Create from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < BLOCK_SIZE {
            return Err(CompressionError::InvalidData);
        }

        let header = unsafe { core::ptr::read(bytes.as_ptr() as *const Self) };

        // Verify it's a valid ustar header
        if &header.magic != b"ustar\0" && &header.magic[..5] != b"ustar" {
            return Err(CompressionError::InvalidData);
        }

        Ok(header)
    }
}

impl Default for TarHeader {
    fn default() -> Self {
        Self::new()
    }
}

/// TAR entry (header + data)
#[derive(Debug, Clone)]
pub struct TarEntry {
    pub header: TarHeader,
    pub data: Vec<u8>,
}

impl TarEntry {
    /// Create a new entry
    pub fn new(name: &str, data: Vec<u8>, file_type: FileType) -> Self {
        let mut header = TarHeader::new();
        header.set_name(name);
        header.set_size(data.len() as u64);
        header.typeflag = file_type.to_flag();
        header.update_checksum();

        Self { header, data }
    }

    /// Get the file name
    pub fn name(&self) -> Result<String> {
        self.header.get_name()
    }

    /// Get the file type
    pub fn file_type(&self) -> FileType {
        FileType::from_flag(self.header.typeflag)
    }

    /// Get the file size
    pub fn size(&self) -> u64 {
        self.header.get_size().unwrap_or(0)
    }

    /// Write entry to bytes (header + data + padding)
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        // Header
        bytes.extend_from_slice(self.header.to_bytes());

        // Data
        bytes.extend_from_slice(&self.data);

        // Padding to block size
        let padding = (BLOCK_SIZE - (self.data.len() % BLOCK_SIZE)) % BLOCK_SIZE;
        bytes.resize(bytes.len() + padding, 0);

        bytes
    }
}

/// TAR archive builder
pub struct TarBuilder {
    entries: Vec<TarEntry>,
}

impl TarBuilder {
    /// Create a new TAR builder
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Add a file
    pub fn add_file(&mut self, name: &str, data: Vec<u8>) {
        self.entries.push(TarEntry::new(name, data, FileType::Regular));
    }

    /// Add a directory
    pub fn add_directory(&mut self, name: &str) {
        self.entries.push(TarEntry::new(name, Vec::new(), FileType::Directory));
    }

    /// Build the TAR archive
    pub fn build(&self) -> Vec<u8> {
        let mut archive = Vec::new();

        for entry in &self.entries {
            archive.extend_from_slice(&entry.to_bytes());
        }

        // Add two null blocks at the end
        archive.resize(archive.len() + BLOCK_SIZE * 2, 0);

        archive
    }
}

impl Default for TarBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// TAR archive reader
pub struct TarReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> TarReader<'a> {
    /// Create a new TAR reader
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// Read the next entry
    pub fn next_entry(&mut self) -> Result<Option<TarEntry>> {
        // Check if we reached the end (two null blocks)
        if self.pos + BLOCK_SIZE * 2 > self.data.len() {
            return Ok(None);
        }

        // Check for null block (end of archive)
        let block = &self.data[self.pos..self.pos + BLOCK_SIZE];
        if block.iter().all(|&b| b == 0) {
            return Ok(None);
        }

        // Parse header
        let header = TarHeader::from_bytes(block)?;
        self.pos += BLOCK_SIZE;

        // Verify checksum
        if !header.verify_checksum() {
            return Err(CompressionError::ChecksumMismatch);
        }

        // Read data
        let size = header.get_size()? as usize;
        if self.pos + size > self.data.len() {
            return Err(CompressionError::InvalidData);
        }

        let data = self.data[self.pos..self.pos + size].to_vec();
        self.pos += size;

        // Skip padding
        let padding = (BLOCK_SIZE - (size % BLOCK_SIZE)) % BLOCK_SIZE;
        self.pos += padding;

        Ok(Some(TarEntry { header, data }))
    }

    /// Extract all entries
    pub fn entries(&mut self) -> Result<Vec<TarEntry>> {
        let mut entries = Vec::new();

        while let Some(entry) = self.next_entry()? {
            entries.push(entry);
        }

        Ok(entries)
    }
}

/// Helper: Write an octal number to a byte buffer
fn write_octal(buf: &mut [u8], value: u64) {
    let s = alloc::format!("{:0width$o}", value, width = buf.len() - 1);
    let bytes = s.as_bytes();
    let len = bytes.len().min(buf.len() - 1);
    buf[..len].copy_from_slice(&bytes[..len]);
    if len < buf.len() {
        buf[len] = 0;
    }
}

/// Helper: Parse an octal number from a byte buffer
fn parse_octal(buf: &[u8]) -> Result<u64> {
    // Find the end (null or space)
    let end = buf.iter().position(|&b| b == 0 || b == b' ').unwrap_or(buf.len());

    // Parse octal
    let s = str::from_utf8(&buf[..end]).map_err(|_| CompressionError::InvalidData)?;
    u64::from_str_radix(s.trim(), 8).map_err(|_| CompressionError::InvalidData)
}
