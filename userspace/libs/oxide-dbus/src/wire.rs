//! D-Bus Wire Protocol — Marshalling and Unmarshalling
//!
//! — PatchBay: The byte-level guts. Every D-Bus value has alignment requirements
//! (1/2/4/8 bytes depending on type) and a specific encoding. Strings are
//! length-prefixed NUL-terminated. Arrays are length-prefixed with 4-byte alignment.
//! Structs are 8-byte aligned. Get any of this wrong and the bus daemon drops you.

use alloc::string::String;
use alloc::vec::Vec;

/// Alignment helper — round up to boundary.
pub fn align_to(offset: usize, alignment: usize) -> usize {
    (offset + alignment - 1) & !(alignment - 1)
}

/// Marshal a D-Bus value into a byte buffer, respecting alignment.
pub struct Marshaller {
    pub data: Vec<u8>,
    /// Current byte order: b'l' = little-endian, b'B' = big-endian
    pub endian: u8,
}

impl Marshaller {
    pub fn new_le() -> Self {
        Marshaller {
            data: Vec::new(),
            endian: b'l',
        }
    }

    /// Pad to alignment boundary
    pub fn pad_to(&mut self, alignment: usize) {
        let aligned = align_to(self.data.len(), alignment);
        while self.data.len() < aligned {
            self.data.push(0);
        }
    }

    /// Write a BYTE (y)
    pub fn write_byte(&mut self, v: u8) {
        self.data.push(v);
    }

    /// Write a BOOLEAN (b) — 4 bytes, 0 or 1
    pub fn write_boolean(&mut self, v: bool) {
        self.pad_to(4);
        let val: u32 = if v { 1 } else { 0 };
        self.data.extend_from_slice(&val.to_le_bytes());
    }

    /// Write an INT32 (i) / UINT32 (u)
    pub fn write_u32(&mut self, v: u32) {
        self.pad_to(4);
        self.data.extend_from_slice(&v.to_le_bytes());
    }

    pub fn write_i32(&mut self, v: i32) {
        self.pad_to(4);
        self.data.extend_from_slice(&v.to_le_bytes());
    }

    /// Write an INT64 (x) / UINT64 (t)
    pub fn write_u64(&mut self, v: u64) {
        self.pad_to(8);
        self.data.extend_from_slice(&v.to_le_bytes());
    }

    /// Write a STRING (s) or OBJECT_PATH (o) — length(u32) + data + NUL
    pub fn write_string(&mut self, s: &str) {
        self.pad_to(4);
        let len = s.len() as u32;
        self.data.extend_from_slice(&len.to_le_bytes());
        self.data.extend_from_slice(s.as_bytes());
        self.data.push(0); // NUL terminator
    }

    /// Write a SIGNATURE (g) — length(u8) + data + NUL
    pub fn write_signature(&mut self, s: &str) {
        let len = s.len() as u8;
        self.data.push(len);
        self.data.extend_from_slice(s.as_bytes());
        self.data.push(0);
    }

    /// Write a VARIANT (v) — signature + value
    pub fn write_variant_string(&mut self, s: &str) {
        self.write_signature("s");
        self.write_string(s);
    }

    pub fn write_variant_u32(&mut self, v: u32) {
        self.write_signature("u");
        self.write_u32(v);
    }

    /// Get current position (for array length fixup)
    pub fn pos(&self) -> usize {
        self.data.len()
    }

    /// Reserve space for array length (returns position of the length field)
    pub fn begin_array(&mut self) -> usize {
        self.pad_to(4);
        let pos = self.data.len();
        self.data.extend_from_slice(&0u32.to_le_bytes()); // placeholder
        pos
    }

    /// Fix up array length at the position returned by begin_array
    pub fn end_array(&mut self, length_pos: usize, content_start: usize) {
        let length = (self.data.len() - content_start) as u32;
        self.data[length_pos..length_pos + 4].copy_from_slice(&length.to_le_bytes());
    }
}

/// Unmarshal D-Bus values from a byte buffer.
pub struct Unmarshaller<'a> {
    pub data: &'a [u8],
    pub pos: usize,
    pub endian: u8,
}

impl<'a> Unmarshaller<'a> {
    pub fn new(data: &'a [u8], endian: u8) -> Self {
        Unmarshaller { data, pos: 0, endian }
    }

    pub fn remaining(&self) -> usize {
        if self.pos < self.data.len() {
            self.data.len() - self.pos
        } else {
            0
        }
    }

    pub fn pad_to(&mut self, alignment: usize) {
        self.pos = align_to(self.pos, alignment);
    }

    pub fn read_byte(&mut self) -> Option<u8> {
        if self.pos < self.data.len() {
            let v = self.data[self.pos];
            self.pos += 1;
            Some(v)
        } else {
            None
        }
    }

    pub fn read_boolean(&mut self) -> Option<bool> {
        self.pad_to(4);
        let v = self.read_u32()?;
        Some(v != 0)
    }

    pub fn read_u32(&mut self) -> Option<u32> {
        self.pad_to(4);
        if self.pos + 4 > self.data.len() {
            return None;
        }
        let bytes: [u8; 4] = self.data[self.pos..self.pos + 4].try_into().ok()?;
        self.pos += 4;
        if self.endian == b'l' {
            Some(u32::from_le_bytes(bytes))
        } else {
            Some(u32::from_be_bytes(bytes))
        }
    }

    pub fn read_i32(&mut self) -> Option<i32> {
        self.read_u32().map(|v| v as i32)
    }

    pub fn read_u64(&mut self) -> Option<u64> {
        self.pad_to(8);
        if self.pos + 8 > self.data.len() {
            return None;
        }
        let bytes: [u8; 8] = self.data[self.pos..self.pos + 8].try_into().ok()?;
        self.pos += 8;
        if self.endian == b'l' {
            Some(u64::from_le_bytes(bytes))
        } else {
            Some(u64::from_be_bytes(bytes))
        }
    }

    pub fn read_string(&mut self) -> Option<String> {
        self.pad_to(4);
        let len = self.read_u32()? as usize;
        if self.pos + len + 1 > self.data.len() {
            return None;
        }
        let s = core::str::from_utf8(&self.data[self.pos..self.pos + len]).ok()?;
        self.pos += len + 1; // +1 for NUL
        Some(String::from(s))
    }

    pub fn read_signature(&mut self) -> Option<String> {
        let len = self.read_byte()? as usize;
        if self.pos + len + 1 > self.data.len() {
            return None;
        }
        let s = core::str::from_utf8(&self.data[self.pos..self.pos + len]).ok()?;
        self.pos += len + 1;
        Some(String::from(s))
    }

    /// Read array length and return (length, content_start_pos)
    pub fn read_array_length(&mut self) -> Option<(usize, usize)> {
        self.pad_to(4);
        let len = self.read_u32()? as usize;
        Some((len, self.pos))
    }

    /// Skip N bytes
    pub fn skip(&mut self, n: usize) {
        self.pos += n;
    }
}
