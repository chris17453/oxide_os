//! DEFLATE compression/decompression
//!
//! Provides DEFLATE (RFC 1951) and GZIP (RFC 1952) support.

use crate::{deflate_impl, CompressionError, CompressionLevel, Result};
use alloc::vec::Vec;

/// GZIP header magic bytes
const GZIP_MAGIC: [u8; 2] = [0x1f, 0x8b];

/// GZIP compression method (8 = DEFLATE)
const GZIP_METHOD_DEFLATE: u8 = 8;

/// GZIP flags
pub mod gzip_flags {
    pub const FTEXT: u8 = 0x01; // Text file
    pub const FHCRC: u8 = 0x02; // Header CRC
    pub const FEXTRA: u8 = 0x04; // Extra fields
    pub const FNAME: u8 = 0x08; // Original filename
    pub const FCOMMENT: u8 = 0x10; // Comment
}

/// GZIP header structure
#[derive(Debug, Clone)]
pub struct GzipHeader {
    pub filename: Option<Vec<u8>>,
    pub comment: Option<Vec<u8>>,
    pub mtime: u32,
    pub os: u8,
}

impl Default for GzipHeader {
    fn default() -> Self {
        Self {
            filename: None,
            comment: None,
            mtime: 0,
            os: 3, // Unix
        }
    }
}

/// Compress data using DEFLATE algorithm
///
/// # Arguments
/// * `input` - Uncompressed data
/// * `level` - Compression level (0-9)
///
/// # Returns
/// Compressed data
pub fn deflate(input: &[u8], level: CompressionLevel) -> Result<Vec<u8>> {
    deflate_impl::compress_deflate(input, level.value())
}

/// Decompress DEFLATE data
///
/// # Arguments
/// * `input` - Compressed data
///
/// # Returns
/// Decompressed data
pub fn inflate(input: &[u8]) -> Result<Vec<u8>> {
    if input.is_empty() {
        return Err(CompressionError::InvalidData);
    }

    deflate_impl::decompress_deflate(input)
}

/// Compress data to GZIP format
///
/// # Arguments
/// * `input` - Uncompressed data
/// * `level` - Compression level
/// * `header` - GZIP header information
///
/// # Returns
/// GZIP compressed data
pub fn gzip_compress(
    input: &[u8],
    level: CompressionLevel,
    header: &GzipHeader,
) -> Result<Vec<u8>> {
    let mut output = Vec::new();

    // GZIP header
    output.extend_from_slice(&GZIP_MAGIC);
    output.push(GZIP_METHOD_DEFLATE);

    // Flags
    let mut flags = 0u8;
    if header.filename.is_some() {
        flags |= gzip_flags::FNAME;
    }
    if header.comment.is_some() {
        flags |= gzip_flags::FCOMMENT;
    }
    output.push(flags);

    // Modification time
    output.extend_from_slice(&header.mtime.to_le_bytes());

    // Extra flags (2 = max compression, 4 = fastest)
    output.push(if level.value() >= 9 { 2 } else { 0 });

    // OS
    output.push(header.os);

    // Optional fields
    if let Some(ref filename) = header.filename {
        output.extend_from_slice(filename);
        output.push(0); // Null terminator
    }

    if let Some(ref comment) = header.comment {
        output.extend_from_slice(comment);
        output.push(0); // Null terminator
    }

    // Compressed data
    let compressed = deflate(input, level)?;
    output.extend_from_slice(&compressed);

    // CRC32 of uncompressed data
    let crc = crc32(input);
    output.extend_from_slice(&crc.to_le_bytes());

    // Size of uncompressed data (mod 2^32)
    output.extend_from_slice(&(input.len() as u32).to_le_bytes());

    Ok(output)
}

/// Decompress GZIP data
///
/// # Arguments
/// * `input` - GZIP compressed data
///
/// # Returns
/// Tuple of (decompressed data, header)
pub fn gzip_decompress(input: &[u8]) -> Result<(Vec<u8>, GzipHeader)> {
    if input.len() < 10 {
        return Err(CompressionError::InvalidData);
    }

    // Check magic bytes
    if input[0..2] != GZIP_MAGIC {
        return Err(CompressionError::InvalidData);
    }

    // Check method
    if input[2] != GZIP_METHOD_DEFLATE {
        return Err(CompressionError::UnsupportedFormat);
    }

    let flags = input[3];
    let mtime = u32::from_le_bytes([input[4], input[5], input[6], input[7]]);
    let _xfl = input[8];
    let os = input[9];

    let mut pos = 10;

    // Skip extra field if present
    if flags & gzip_flags::FEXTRA != 0 {
        if input.len() < pos + 2 {
            return Err(CompressionError::InvalidData);
        }
        let xlen = u16::from_le_bytes([input[pos], input[pos + 1]]) as usize;
        pos += 2 + xlen;
    }

    // Read filename if present
    let filename = if flags & gzip_flags::FNAME != 0 {
        let start = pos;
        while pos < input.len() && input[pos] != 0 {
            pos += 1;
        }
        if pos >= input.len() {
            return Err(CompressionError::InvalidData);
        }
        let name = input[start..pos].to_vec();
        pos += 1; // Skip null terminator
        Some(name)
    } else {
        None
    };

    // Read comment if present
    let comment = if flags & gzip_flags::FCOMMENT != 0 {
        let start = pos;
        while pos < input.len() && input[pos] != 0 {
            pos += 1;
        }
        if pos >= input.len() {
            return Err(CompressionError::InvalidData);
        }
        let cmt = input[start..pos].to_vec();
        pos += 1; // Skip null terminator
        Some(cmt)
    } else {
        None
    };

    // Skip header CRC if present
    if flags & gzip_flags::FHCRC != 0 {
        pos += 2;
    }

    if input.len() < pos + 8 {
        return Err(CompressionError::InvalidData);
    }

    // Decompress data (everything except last 8 bytes which are CRC and size)
    let compressed_data = &input[pos..input.len() - 8];
    let decompressed = inflate(compressed_data)?;

    // Verify CRC32
    let expected_crc = u32::from_le_bytes([
        input[input.len() - 8],
        input[input.len() - 7],
        input[input.len() - 6],
        input[input.len() - 5],
    ]);
    let actual_crc = crc32(&decompressed);
    if actual_crc != expected_crc {
        return Err(CompressionError::ChecksumMismatch);
    }

    // Verify size
    let expected_size = u32::from_le_bytes([
        input[input.len() - 4],
        input[input.len() - 3],
        input[input.len() - 2],
        input[input.len() - 1],
    ]);
    if (decompressed.len() as u32) != expected_size {
        return Err(CompressionError::InvalidData);
    }

    let header = GzipHeader {
        filename,
        comment,
        mtime,
        os,
    };

    Ok((decompressed, header))
}

/// CRC32 calculation (polynomial 0xEDB88320)
fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFFFFFF_u32;

    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
        }
    }

    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crc32() {
        // Test vector from RFC 1952
        let data = b"hello world";
        let crc = crc32(data);
        assert_eq!(crc, 0x0D4A1185);
    }
}
