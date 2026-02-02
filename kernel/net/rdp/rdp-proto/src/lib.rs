//! RDP Protocol Parsing
//!
//! This crate implements parsing and serialization for the RDP protocol stack:
//! - TPKT (RFC 1006) - Transport layer framing
//! - X.224 (ISO 8073) - Connection-oriented transport protocol
//! - MCS (T.125) - Multipoint Communication Service
//! - RDP PDUs - Application layer messages

#![no_std]

extern crate alloc;

pub mod ber;
pub mod fast_path;
pub mod gcc;
pub mod mcs;
pub mod pdu;
pub mod tpkt;
pub mod x224;

use alloc::vec::Vec;
use rdp_traits::{RdpError, RdpResult};

/// Cursor for reading binary data
pub struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    /// Create a new cursor
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// Get remaining bytes
    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    /// Get current position
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Check if cursor is at end
    pub fn is_empty(&self) -> bool {
        self.pos >= self.data.len()
    }

    /// Get slice of remaining data
    pub fn as_slice(&self) -> &'a [u8] {
        &self.data[self.pos..]
    }

    /// Read a single byte
    pub fn read_u8(&mut self) -> RdpResult<u8> {
        if self.remaining() < 1 {
            return Err(RdpError::InsufficientData);
        }
        let val = self.data[self.pos];
        self.pos += 1;
        Ok(val)
    }

    /// Read a big-endian u16
    pub fn read_u16_be(&mut self) -> RdpResult<u16> {
        if self.remaining() < 2 {
            return Err(RdpError::InsufficientData);
        }
        let val = u16::from_be_bytes([self.data[self.pos], self.data[self.pos + 1]]);
        self.pos += 2;
        Ok(val)
    }

    /// Read a little-endian u16
    pub fn read_u16_le(&mut self) -> RdpResult<u16> {
        if self.remaining() < 2 {
            return Err(RdpError::InsufficientData);
        }
        let val = u16::from_le_bytes([self.data[self.pos], self.data[self.pos + 1]]);
        self.pos += 2;
        Ok(val)
    }

    /// Read a big-endian u32
    pub fn read_u32_be(&mut self) -> RdpResult<u32> {
        if self.remaining() < 4 {
            return Err(RdpError::InsufficientData);
        }
        let val = u32::from_be_bytes([
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
        ]);
        self.pos += 4;
        Ok(val)
    }

    /// Read a little-endian u32
    pub fn read_u32_le(&mut self) -> RdpResult<u32> {
        if self.remaining() < 4 {
            return Err(RdpError::InsufficientData);
        }
        let val = u32::from_le_bytes([
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
        ]);
        self.pos += 4;
        Ok(val)
    }

    /// Read exact number of bytes
    pub fn read_bytes(&mut self, len: usize) -> RdpResult<&'a [u8]> {
        if self.remaining() < len {
            return Err(RdpError::InsufficientData);
        }
        let slice = &self.data[self.pos..self.pos + len];
        self.pos += len;
        Ok(slice)
    }

    /// Skip bytes
    pub fn skip(&mut self, len: usize) -> RdpResult<()> {
        if self.remaining() < len {
            return Err(RdpError::InsufficientData);
        }
        self.pos += len;
        Ok(())
    }

    /// Peek at next byte without advancing
    pub fn peek_u8(&self) -> RdpResult<u8> {
        if self.remaining() < 1 {
            return Err(RdpError::InsufficientData);
        }
        Ok(self.data[self.pos])
    }
}

/// Writer for building binary data
pub struct Writer {
    data: Vec<u8>,
}

impl Writer {
    /// Create a new writer
    pub fn new() -> Self {
        Self { data: Vec::new() }
    }

    /// Create a writer with capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: Vec::with_capacity(capacity),
        }
    }

    /// Get current length
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Write a single byte
    pub fn write_u8(&mut self, val: u8) {
        self.data.push(val);
    }

    /// Write a big-endian u16
    pub fn write_u16_be(&mut self, val: u16) {
        self.data.extend_from_slice(&val.to_be_bytes());
    }

    /// Write a little-endian u16
    pub fn write_u16_le(&mut self, val: u16) {
        self.data.extend_from_slice(&val.to_le_bytes());
    }

    /// Write a big-endian u32
    pub fn write_u32_be(&mut self, val: u32) {
        self.data.extend_from_slice(&val.to_be_bytes());
    }

    /// Write a little-endian u32
    pub fn write_u32_le(&mut self, val: u32) {
        self.data.extend_from_slice(&val.to_le_bytes());
    }

    /// Write bytes
    pub fn write_bytes(&mut self, bytes: &[u8]) {
        self.data.extend_from_slice(bytes);
    }

    /// Write padding bytes
    pub fn write_padding(&mut self, len: usize) {
        self.data.resize(self.data.len() + len, 0);
    }

    /// Get the written data
    pub fn into_vec(self) -> Vec<u8> {
        self.data
    }

    /// Get reference to written data
    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }

    /// Set byte at position
    pub fn set_u8(&mut self, pos: usize, val: u8) {
        if pos < self.data.len() {
            self.data[pos] = val;
        }
    }

    /// Set big-endian u16 at position
    pub fn set_u16_be(&mut self, pos: usize, val: u16) {
        let bytes = val.to_be_bytes();
        if pos + 1 < self.data.len() {
            self.data[pos] = bytes[0];
            self.data[pos + 1] = bytes[1];
        }
    }
}

impl Default for Writer {
    fn default() -> Self {
        Self::new()
    }
}
