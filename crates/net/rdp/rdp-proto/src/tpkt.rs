//! TPKT (RFC 1006) - Transport layer framing
//!
//! TPKT provides a simple encapsulation of ISO transport protocol data units
//! (TPDUs) over TCP. Each TPKT packet has a 4-byte header:
//!
//! ```text
//! +--------+--------+--------+--------+
//! | version| reserved|     length      |
//! +--------+--------+--------+--------+
//! |           TPDU data ...           |
//! +--------+--------+--------+--------+
//! ```

use crate::{Cursor, Writer};
use rdp_traits::{protocol, RdpError, RdpResult};
use alloc::vec::Vec;

/// TPKT header
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TpktHeader {
    /// Protocol version (always 3)
    pub version: u8,
    /// Total packet length including header
    pub length: u16,
}

impl TpktHeader {
    /// Header size in bytes
    pub const SIZE: usize = 4;

    /// Parse a TPKT header from bytes
    pub fn parse(cursor: &mut Cursor<'_>) -> RdpResult<Self> {
        let version = cursor.read_u8()?;
        if version != protocol::TPKT_VERSION {
            return Err(RdpError::InvalidProtocol);
        }

        let _reserved = cursor.read_u8()?;
        let length = cursor.read_u16_be()?;

        if length < Self::SIZE as u16 {
            return Err(RdpError::InvalidProtocol);
        }

        Ok(Self { version, length })
    }

    /// Get the payload length (total length minus header)
    pub fn payload_length(&self) -> usize {
        (self.length as usize).saturating_sub(Self::SIZE)
    }

    /// Write a TPKT header
    pub fn write(&self, writer: &mut Writer) {
        writer.write_u8(self.version);
        writer.write_u8(0); // reserved
        writer.write_u16_be(self.length);
    }
}

/// TPKT packet
#[derive(Debug, Clone)]
pub struct TpktPacket {
    /// Packet payload (TPDU)
    pub payload: Vec<u8>,
}

impl TpktPacket {
    /// Create a new TPKT packet
    pub fn new(payload: Vec<u8>) -> Self {
        Self { payload }
    }

    /// Parse a TPKT packet from a stream
    ///
    /// Returns `None` if more data is needed (incomplete packet)
    /// Returns `Some(packet, bytes_consumed)` on success
    pub fn parse(data: &[u8]) -> RdpResult<Option<(Self, usize)>> {
        if data.len() < TpktHeader::SIZE {
            return Ok(None); // Need more data
        }

        let mut cursor = Cursor::new(data);
        let header = TpktHeader::parse(&mut cursor)?;

        let total_len = header.length as usize;
        if data.len() < total_len {
            return Ok(None); // Need more data
        }

        let payload = cursor.read_bytes(header.payload_length())?.to_vec();

        Ok(Some((Self { payload }, total_len)))
    }

    /// Encode a TPKT packet
    pub fn encode(&self) -> Vec<u8> {
        let total_len = TpktHeader::SIZE + self.payload.len();
        let mut writer = Writer::with_capacity(total_len);

        let header = TpktHeader {
            version: protocol::TPKT_VERSION,
            length: total_len as u16,
        };
        header.write(&mut writer);
        writer.write_bytes(&self.payload);

        writer.into_vec()
    }
}

/// Encode data into a TPKT packet
///
/// This is a convenience function that wraps data in TPKT framing.
pub fn encode(data: &[u8]) -> Vec<u8> {
    TpktPacket::new(data.to_vec()).encode()
}

/// Check if data starts with a valid TPKT header
pub fn is_tpkt(data: &[u8]) -> bool {
    data.len() >= 4 && data[0] == protocol::TPKT_VERSION
}

/// Get the expected packet length from a TPKT header
///
/// Returns `None` if data is too short or invalid
pub fn peek_length(data: &[u8]) -> Option<u16> {
    if data.len() >= 4 && data[0] == protocol::TPKT_VERSION {
        Some(u16::from_be_bytes([data[2], data[3]]))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tpkt_encode_decode() {
        let payload = vec![1, 2, 3, 4, 5];
        let packet = TpktPacket::new(payload.clone());
        let encoded = packet.encode();

        assert_eq!(encoded[0], 3); // version
        assert_eq!(encoded[1], 0); // reserved
        assert_eq!(u16::from_be_bytes([encoded[2], encoded[3]]), 9); // length = 4 + 5

        let (decoded, consumed) = TpktPacket::parse(&encoded).unwrap().unwrap();
        assert_eq!(consumed, 9);
        assert_eq!(decoded.payload, payload);
    }

    #[test]
    fn test_tpkt_incomplete() {
        let data = vec![3, 0, 0, 20]; // Header says 20 bytes but only 4 present
        assert!(TpktPacket::parse(&data).unwrap().is_none());
    }
}
