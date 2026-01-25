//! BER (Basic Encoding Rules) and PER (Packed Encoding Rules) Support
//!
//! MCS uses ASN.1 BER for the connect-initial/response PDUs and a
//! simplified form similar to PER for other messages.

use crate::{Cursor, Writer};
use rdp_traits::{RdpError, RdpResult};

/// BER tag classes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagClass {
    Universal,
    Application,
    ContextSpecific,
    Private,
}

/// BER tag types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagType {
    Primitive,
    Constructed,
}

/// Common BER universal tags
pub mod tags {
    pub const BOOLEAN: u8 = 0x01;
    pub const INTEGER: u8 = 0x02;
    pub const BIT_STRING: u8 = 0x03;
    pub const OCTET_STRING: u8 = 0x04;
    pub const OBJECT_IDENTIFIER: u8 = 0x06;
    pub const ENUMERATED: u8 = 0x0A;
    pub const SEQUENCE: u8 = 0x30;
    pub const SET: u8 = 0x31;
}

/// BER Tag
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tag {
    pub class: TagClass,
    pub tag_type: TagType,
    pub number: u32,
}

impl Tag {
    /// Create a context-specific tag
    pub const fn context(number: u32) -> Self {
        Self {
            class: TagClass::ContextSpecific,
            tag_type: TagType::Constructed,
            number,
        }
    }

    /// Create a universal tag
    pub const fn universal(number: u32) -> Self {
        Self {
            class: TagClass::Universal,
            tag_type: TagType::Primitive,
            number,
        }
    }

    /// Create an application tag
    pub const fn application(number: u32) -> Self {
        Self {
            class: TagClass::Application,
            tag_type: TagType::Constructed,
            number,
        }
    }

    /// Encode the tag byte(s)
    pub fn encode(&self) -> u8 {
        let class_bits = match self.class {
            TagClass::Universal => 0x00,
            TagClass::Application => 0x40,
            TagClass::ContextSpecific => 0x80,
            TagClass::Private => 0xC0,
        };

        let type_bit = match self.tag_type {
            TagType::Primitive => 0x00,
            TagType::Constructed => 0x20,
        };

        class_bits | type_bit | (self.number as u8 & 0x1F)
    }

    /// Parse a tag byte
    pub fn parse(byte: u8) -> Self {
        let class = match byte & 0xC0 {
            0x00 => TagClass::Universal,
            0x40 => TagClass::Application,
            0x80 => TagClass::ContextSpecific,
            _ => TagClass::Private,
        };

        let tag_type = if byte & 0x20 != 0 {
            TagType::Constructed
        } else {
            TagType::Primitive
        };

        let number = (byte & 0x1F) as u32;

        Self {
            class,
            tag_type,
            number,
        }
    }
}

/// Read a BER length field
pub fn read_length(cursor: &mut Cursor<'_>) -> RdpResult<usize> {
    let first = cursor.read_u8()?;

    if first < 0x80 {
        // Short form: single byte
        Ok(first as usize)
    } else if first == 0x80 {
        // Indefinite length (not supported in RDP)
        Err(RdpError::InvalidProtocol)
    } else {
        // Long form: first byte indicates number of length bytes
        let num_bytes = (first & 0x7F) as usize;
        if num_bytes > 4 {
            return Err(RdpError::InvalidProtocol);
        }

        let mut length: usize = 0;
        for _ in 0..num_bytes {
            length = (length << 8) | cursor.read_u8()? as usize;
        }
        Ok(length)
    }
}

/// Write a BER length field
pub fn write_length(writer: &mut Writer, length: usize) {
    if length < 0x80 {
        // Short form
        writer.write_u8(length as u8);
    } else if length <= 0xFF {
        // Long form, 1 byte
        writer.write_u8(0x81);
        writer.write_u8(length as u8);
    } else if length <= 0xFFFF {
        // Long form, 2 bytes
        writer.write_u8(0x82);
        writer.write_u16_be(length as u16);
    } else {
        // Long form, 4 bytes
        writer.write_u8(0x84);
        writer.write_u32_be(length as u32);
    }
}

/// Read a BER integer
pub fn read_integer(cursor: &mut Cursor<'_>) -> RdpResult<i64> {
    let tag = cursor.read_u8()?;
    if tag != tags::INTEGER {
        return Err(RdpError::InvalidProtocol);
    }

    let length = read_length(cursor)?;
    if length == 0 || length > 8 {
        return Err(RdpError::InvalidProtocol);
    }

    let bytes = cursor.read_bytes(length)?;

    // Sign-extend if negative
    let mut value: i64 = if bytes[0] & 0x80 != 0 { -1 } else { 0 };

    for &byte in bytes {
        value = (value << 8) | byte as i64;
    }

    Ok(value)
}

/// Write a BER integer
pub fn write_integer(writer: &mut Writer, value: i64) {
    writer.write_u8(tags::INTEGER);

    // Determine minimum bytes needed
    let bytes = if value >= 0 {
        if value <= 0x7F {
            1
        } else if value <= 0x7FFF {
            2
        } else if value <= 0x7FFFFF {
            3
        } else if value <= 0x7FFFFFFF {
            4
        } else {
            8
        }
    } else {
        if value >= -0x80 {
            1
        } else if value >= -0x8000 {
            2
        } else if value >= -0x800000 {
            3
        } else if value >= -0x80000000 {
            4
        } else {
            8
        }
    };

    write_length(writer, bytes);

    let value_bytes = value.to_be_bytes();
    writer.write_bytes(&value_bytes[8 - bytes..]);
}

/// Read a BER unsigned integer (as used in RDP)
pub fn read_unsigned(cursor: &mut Cursor<'_>) -> RdpResult<u32> {
    let tag = cursor.read_u8()?;
    if tag != tags::INTEGER {
        return Err(RdpError::InvalidProtocol);
    }

    let length = read_length(cursor)?;
    if length == 0 || length > 4 {
        // Allow an extra leading zero byte for unsigned values
        if length == 5 {
            let leading = cursor.read_u8()?;
            if leading != 0 {
                return Err(RdpError::InvalidProtocol);
            }
            let bytes = cursor.read_bytes(4)?;
            return Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]));
        }
        return Err(RdpError::InvalidProtocol);
    }

    let bytes = cursor.read_bytes(length)?;
    let mut value: u32 = 0;
    for &byte in bytes {
        value = (value << 8) | byte as u32;
    }

    Ok(value)
}

/// Write a BER unsigned integer
pub fn write_unsigned(writer: &mut Writer, value: u32) {
    writer.write_u8(tags::INTEGER);

    // Determine minimum bytes needed (with leading zero if high bit set)
    let (bytes, needs_pad) = if value == 0 {
        (1, false)
    } else if value <= 0x7F {
        (1, false)
    } else if value <= 0xFF {
        (1, true) // Need padding to avoid sign extension
    } else if value <= 0x7FFF {
        (2, false)
    } else if value <= 0xFFFF {
        (2, true)
    } else if value <= 0x7FFFFF {
        (3, false)
    } else if value <= 0xFFFFFF {
        (3, true)
    } else if value <= 0x7FFFFFFF {
        (4, false)
    } else {
        (4, true)
    };

    let total_len = bytes + if needs_pad { 1 } else { 0 };
    write_length(writer, total_len);

    if needs_pad {
        writer.write_u8(0);
    }

    let value_bytes = value.to_be_bytes();
    writer.write_bytes(&value_bytes[4 - bytes..]);
}

/// Read a BER octet string
pub fn read_octet_string<'a>(cursor: &mut Cursor<'a>) -> RdpResult<&'a [u8]> {
    let tag = cursor.read_u8()?;
    if tag != tags::OCTET_STRING {
        return Err(RdpError::InvalidProtocol);
    }

    let length = read_length(cursor)?;
    cursor.read_bytes(length)
}

/// Write a BER octet string
pub fn write_octet_string(writer: &mut Writer, data: &[u8]) {
    writer.write_u8(tags::OCTET_STRING);
    write_length(writer, data.len());
    writer.write_bytes(data);
}

/// Read a BER enumerated value
pub fn read_enumerated(cursor: &mut Cursor<'_>) -> RdpResult<u8> {
    let tag = cursor.read_u8()?;
    if tag != tags::ENUMERATED {
        return Err(RdpError::InvalidProtocol);
    }

    let length = read_length(cursor)?;
    if length != 1 {
        return Err(RdpError::InvalidProtocol);
    }

    cursor.read_u8()
}

/// Write a BER enumerated value
pub fn write_enumerated(writer: &mut Writer, value: u8) {
    writer.write_u8(tags::ENUMERATED);
    write_length(writer, 1);
    writer.write_u8(value);
}

/// Read a BER boolean
pub fn read_boolean(cursor: &mut Cursor<'_>) -> RdpResult<bool> {
    let tag = cursor.read_u8()?;
    if tag != tags::BOOLEAN {
        return Err(RdpError::InvalidProtocol);
    }

    let length = read_length(cursor)?;
    if length != 1 {
        return Err(RdpError::InvalidProtocol);
    }

    Ok(cursor.read_u8()? != 0)
}

/// Write a BER boolean
pub fn write_boolean(writer: &mut Writer, value: bool) {
    writer.write_u8(tags::BOOLEAN);
    write_length(writer, 1);
    writer.write_u8(if value { 0xFF } else { 0x00 });
}

/// Calculate the length of a BER length field
pub fn length_size(length: usize) -> usize {
    if length < 0x80 {
        1
    } else if length <= 0xFF {
        2
    } else if length <= 0xFFFF {
        3
    } else {
        5
    }
}

/// Calculate the total size of a BER TLV with given content length
pub fn tlv_size(content_length: usize) -> usize {
    1 + length_size(content_length) + content_length
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Writer;

    #[test]
    fn test_length_encoding() {
        let mut writer = Writer::new();
        write_length(&mut writer, 10);
        assert_eq!(writer.as_slice(), &[10]);

        let mut writer = Writer::new();
        write_length(&mut writer, 200);
        assert_eq!(writer.as_slice(), &[0x81, 200]);

        let mut writer = Writer::new();
        write_length(&mut writer, 1000);
        assert_eq!(writer.as_slice(), &[0x82, 0x03, 0xE8]);
    }

    #[test]
    fn test_integer_roundtrip() {
        let mut writer = Writer::new();
        write_integer(&mut writer, 42);

        let mut cursor = Cursor::new(writer.as_slice());
        assert_eq!(read_integer(&mut cursor).unwrap(), 42);
    }

    #[test]
    fn test_unsigned_roundtrip() {
        let mut writer = Writer::new();
        write_unsigned(&mut writer, 0x80000000);

        let mut cursor = Cursor::new(writer.as_slice());
        assert_eq!(read_unsigned(&mut cursor).unwrap(), 0x80000000);
    }
}
