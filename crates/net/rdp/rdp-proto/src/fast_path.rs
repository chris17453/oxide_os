//! Fast-Path Input/Output
//!
//! Fast-path provides an optimized encoding for input events and
//! graphics updates, bypassing the X.224/MCS/Share layers for
//! better performance.

use crate::{Cursor, Writer};
use alloc::vec::Vec;
use rdp_traits::{ExtendedMouseFlags, KeyboardFlags, MouseFlags, RdpError, RdpResult};

/// Fast-path input header
#[derive(Debug, Clone, Copy)]
pub struct FastPathInputHeader {
    /// Action (0 = fast-path input)
    pub action: u8,
    /// Number of events
    pub num_events: u8,
    /// Flags (encryption)
    pub flags: u8,
    /// Total length
    pub length: u16,
}

impl FastPathInputHeader {
    /// Parse fast-path input header
    pub fn parse(data: &[u8]) -> RdpResult<Option<(Self, usize)>> {
        if data.is_empty() {
            return Ok(None);
        }

        let first_byte = data[0];

        // Check for fast-path (action field = 0)
        if (first_byte & 0x03) != 0 {
            // Not fast-path
            return Ok(None);
        }

        if data.len() < 2 {
            return Ok(None);
        }

        // Extract fields from first byte
        let num_events = (first_byte >> 2) & 0x0F;
        let flags = (first_byte >> 6) & 0x03;

        // Parse length
        let (length, header_size) = if data[1] & 0x80 != 0 {
            // 2-byte length
            if data.len() < 3 {
                return Ok(None);
            }
            let len = ((data[1] as u16 & 0x7F) << 8) | data[2] as u16;
            (len, 3)
        } else {
            // 1-byte length
            (data[1] as u16, 2)
        };

        if data.len() < length as usize {
            return Ok(None);
        }

        Ok(Some((
            Self {
                action: 0,
                num_events,
                flags,
                length,
            },
            header_size,
        )))
    }
}

/// Fast-path input event types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FastPathInputType {
    /// Keyboard event
    Keyboard = 0x00,
    /// Mouse event
    Mouse = 0x01,
    /// Extended mouse event
    ExtendedMouse = 0x02,
    /// Synchronize event
    Synchronize = 0x03,
    /// Unicode keyboard event
    Unicode = 0x04,
    /// Quality of experience timestamp
    QoeTimestamp = 0x06,
}

/// Fast-path keyboard flags
#[derive(Debug, Clone, Copy)]
pub struct FastPathKeyboardFlags(pub u8);

impl FastPathKeyboardFlags {
    /// Key release
    pub const RELEASE: u8 = 0x01;
    /// Extended key
    pub const EXTENDED: u8 = 0x02;
    /// Extended1 key (Pause/Break)
    pub const EXTENDED1: u8 = 0x04;
}

/// Fast-path input event
#[derive(Debug, Clone)]
pub enum FastPathInputEvent {
    /// Keyboard event
    Keyboard {
        flags: FastPathKeyboardFlags,
        scancode: u8,
    },
    /// Mouse event
    Mouse {
        flags: MouseFlags,
        x: u16,
        y: u16,
    },
    /// Extended mouse event
    ExtendedMouse {
        flags: ExtendedMouseFlags,
        x: u16,
        y: u16,
    },
    /// Synchronize event
    Synchronize { flags: u8 },
    /// Unicode keyboard event
    Unicode { code_point: u16, is_release: bool },
}

impl FastPathInputEvent {
    /// Parse a fast-path input event
    pub fn parse(cursor: &mut Cursor<'_>) -> RdpResult<Self> {
        let header = cursor.read_u8()?;
        let event_type = (header >> 5) & 0x07;
        let event_flags = header & 0x1F;

        match event_type {
            0x00 => {
                // Keyboard
                let scancode = cursor.read_u8()?;
                Ok(FastPathInputEvent::Keyboard {
                    flags: FastPathKeyboardFlags(event_flags),
                    scancode,
                })
            }
            0x01 => {
                // Mouse
                let flags = cursor.read_u16_le()?;
                let x = cursor.read_u16_le()?;
                let y = cursor.read_u16_le()?;
                Ok(FastPathInputEvent::Mouse {
                    flags: MouseFlags::new(flags),
                    x,
                    y,
                })
            }
            0x02 => {
                // Extended mouse
                let flags = cursor.read_u16_le()?;
                let x = cursor.read_u16_le()?;
                let y = cursor.read_u16_le()?;
                Ok(FastPathInputEvent::ExtendedMouse {
                    flags: ExtendedMouseFlags::new(flags),
                    x,
                    y,
                })
            }
            0x03 => {
                // Synchronize
                Ok(FastPathInputEvent::Synchronize {
                    flags: event_flags,
                })
            }
            0x04 => {
                // Unicode
                let code_point = cursor.read_u16_le()?;
                let is_release = event_flags & 0x01 != 0;
                Ok(FastPathInputEvent::Unicode {
                    code_point,
                    is_release,
                })
            }
            _ => Err(RdpError::InvalidProtocol),
        }
    }

    /// Convert to standard RDP input flags
    pub fn to_keyboard_flags(&self) -> Option<(u16, KeyboardFlags)> {
        if let FastPathInputEvent::Keyboard { flags, scancode } = self {
            let mut kf = 0u16;
            if flags.0 & FastPathKeyboardFlags::RELEASE != 0 {
                kf |= KeyboardFlags::RELEASE;
            }
            if flags.0 & FastPathKeyboardFlags::EXTENDED != 0 {
                kf |= KeyboardFlags::EXTENDED;
            }
            if flags.0 & FastPathKeyboardFlags::EXTENDED1 != 0 {
                kf |= KeyboardFlags::EXTENDED1;
            }
            Some((*scancode as u16, KeyboardFlags::new(kf)))
        } else {
            None
        }
    }
}

/// Fast-path output header
#[derive(Debug, Clone, Copy)]
pub struct FastPathOutputHeader {
    /// Action (always 0 for fast-path)
    pub action: u8,
    /// Encryption flags
    pub flags: u8,
    /// Total length
    pub length: u16,
}

/// Fast-path output update types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FastPathUpdateType {
    /// Orders
    Orders = 0x00,
    /// Bitmap
    Bitmap = 0x01,
    /// Palette
    Palette = 0x02,
    /// Synchronize
    Synchronize = 0x03,
    /// Surface commands
    SurfaceCommands = 0x04,
    /// Pointer hidden
    PointerHidden = 0x05,
    /// Pointer default
    PointerDefault = 0x06,
    /// Pointer position
    PointerPosition = 0x08,
    /// Color pointer
    ColorPointer = 0x09,
    /// Cached pointer
    CachedPointer = 0x0A,
    /// New pointer
    NewPointer = 0x0B,
    /// Large pointer
    LargePointer = 0x0C,
}

/// Compression flags for fast-path output
#[derive(Debug, Clone, Copy)]
pub struct FastPathCompressionFlags(pub u8);

impl FastPathCompressionFlags {
    /// No compression
    pub const NONE: u8 = 0x00;
    /// Bulk compression used
    pub const COMPRESSED: u8 = 0x20;
}

/// Fast-path bitmap update
#[derive(Debug, Clone)]
pub struct FastPathBitmapUpdate {
    /// Update rectangles
    pub rectangles: Vec<FastPathBitmapRect>,
}

/// Fast-path bitmap rectangle
#[derive(Debug, Clone)]
pub struct FastPathBitmapRect {
    /// Destination left
    pub dest_left: u16,
    /// Destination top
    pub dest_top: u16,
    /// Destination right
    pub dest_right: u16,
    /// Destination bottom
    pub dest_bottom: u16,
    /// Width
    pub width: u16,
    /// Height
    pub height: u16,
    /// Bits per pixel
    pub bpp: u16,
    /// Compression flags
    pub flags: u16,
    /// Bitmap data (compressed or raw)
    pub data: Vec<u8>,
}

impl FastPathBitmapUpdate {
    /// Encode as fast-path output
    pub fn encode(&self, encryption_flags: u8) -> Vec<u8> {
        // First encode update data
        let mut update_data = Writer::new();

        // Update header (1 byte type + compression)
        update_data.write_u8(FastPathUpdateType::Bitmap as u8);

        // Compression (1 byte, uncompressed for now)
        update_data.write_u8(FastPathCompressionFlags::NONE);

        // Number of rectangles (2 bytes LE)
        update_data.write_u16_le(self.rectangles.len() as u16);

        for rect in &self.rectangles {
            update_data.write_u16_le(rect.dest_left);
            update_data.write_u16_le(rect.dest_top);
            update_data.write_u16_le(rect.dest_right);
            update_data.write_u16_le(rect.dest_bottom);
            update_data.write_u16_le(rect.width);
            update_data.write_u16_le(rect.height);
            update_data.write_u16_le(rect.bpp);
            update_data.write_u16_le(rect.flags);
            update_data.write_u16_le(rect.data.len() as u16);
            update_data.write_bytes(&rect.data);
        }

        let update_bytes = update_data.into_vec();

        // Calculate size for update data length (2-byte PER length)
        let size_field_len = if update_bytes.len() < 0x80 { 1 } else { 2 };

        // Build fast-path packet
        let total_len = 1 + size_field_len + update_bytes.len(); // header + size + data

        let mut writer = Writer::with_capacity(total_len + 2);

        // Fast-path header byte
        // Action (2 bits) = 0, Reserved (2 bits) = 0, Flags (4 bits)
        let header_byte = (encryption_flags & 0x03) << 6;
        writer.write_u8(header_byte);

        // Length (1 or 2 bytes)
        let pdu_length = total_len;
        if pdu_length < 0x80 {
            writer.write_u8(pdu_length as u8);
        } else {
            writer.write_u8(0x80 | ((pdu_length >> 8) as u8 & 0x7F));
            writer.write_u8(pdu_length as u8);
        }

        // Update size (PER encoded)
        if update_bytes.len() < 0x80 {
            writer.write_u8(update_bytes.len() as u8);
        } else {
            writer.write_u8(0x80 | ((update_bytes.len() >> 8) as u8 & 0x7F));
            writer.write_u8(update_bytes.len() as u8);
        }

        // Update data
        writer.write_bytes(&update_bytes);

        writer.into_vec()
    }
}

/// Fast-path pointer update
#[derive(Debug, Clone)]
pub struct FastPathPointerUpdate {
    /// Update type
    pub update_type: FastPathUpdateType,
    /// Pointer data
    pub data: Vec<u8>,
}

impl FastPathPointerUpdate {
    /// Create a pointer position update
    pub fn position(x: u16, y: u16) -> Self {
        let mut data = Writer::new();
        data.write_u16_le(x);
        data.write_u16_le(y);

        Self {
            update_type: FastPathUpdateType::PointerPosition,
            data: data.into_vec(),
        }
    }

    /// Create a hidden pointer update
    pub fn hidden() -> Self {
        Self {
            update_type: FastPathUpdateType::PointerHidden,
            data: Vec::new(),
        }
    }

    /// Create a default pointer update
    pub fn default_pointer() -> Self {
        Self {
            update_type: FastPathUpdateType::PointerDefault,
            data: Vec::new(),
        }
    }

    /// Encode as fast-path output
    pub fn encode(&self, encryption_flags: u8) -> Vec<u8> {
        // Update data: type + compression + data
        let update_len = 2 + self.data.len();
        let size_field_len = if update_len < 0x80 { 1 } else { 2 };
        let total_len = 1 + size_field_len + update_len;

        let mut writer = Writer::with_capacity(total_len + 2);

        // Fast-path header
        let header_byte = (encryption_flags & 0x03) << 6;
        writer.write_u8(header_byte);

        // PDU length
        if total_len < 0x80 {
            writer.write_u8(total_len as u8);
        } else {
            writer.write_u8(0x80 | ((total_len >> 8) as u8 & 0x7F));
            writer.write_u8(total_len as u8);
        }

        // Update size
        if update_len < 0x80 {
            writer.write_u8(update_len as u8);
        } else {
            writer.write_u8(0x80 | ((update_len >> 8) as u8 & 0x7F));
            writer.write_u8(update_len as u8);
        }

        // Update type
        writer.write_u8(self.update_type as u8);

        // Compression (none)
        writer.write_u8(0);

        // Data
        writer.write_bytes(&self.data);

        writer.into_vec()
    }
}

/// Fast-path synchronize update
#[derive(Debug, Clone)]
pub struct FastPathSynchronize;

impl FastPathSynchronize {
    /// Encode as fast-path output
    pub fn encode(encryption_flags: u8) -> Vec<u8> {
        let mut writer = Writer::with_capacity(8);

        // Fast-path header
        let header_byte = (encryption_flags & 0x03) << 6;
        writer.write_u8(header_byte);

        // Length (4 bytes: header + length + size + update)
        writer.write_u8(4);

        // Update size (2 bytes)
        writer.write_u8(2);

        // Update type
        writer.write_u8(FastPathUpdateType::Synchronize as u8);

        // Compression
        writer.write_u8(0);

        writer.into_vec()
    }
}

/// Parse fast-path input events from a packet
pub fn parse_fast_path_input(data: &[u8]) -> RdpResult<Vec<FastPathInputEvent>> {
    let (header, header_size) = match FastPathInputHeader::parse(data)? {
        Some((h, s)) => (h, s),
        None => return Err(RdpError::InvalidProtocol),
    };

    let mut cursor = Cursor::new(&data[header_size..]);
    let mut events = Vec::with_capacity(header.num_events as usize);

    // If num_events is 0, actual count is in first byte of data
    let actual_count = if header.num_events == 0 {
        cursor.read_u8()? as usize
    } else {
        header.num_events as usize
    };

    for _ in 0..actual_count {
        if cursor.is_empty() {
            break;
        }
        events.push(FastPathInputEvent::parse(&mut cursor)?);
    }

    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fast_path_synchronize() {
        let data = FastPathSynchronize::encode(0);
        assert!(data.len() >= 4);
        assert_eq!(data[0] & 0x03, 0); // Action = 0
    }

    #[test]
    fn test_pointer_position() {
        let update = FastPathPointerUpdate::position(100, 200);
        let encoded = update.encode(0);
        assert!(encoded.len() >= 6);
    }
}
