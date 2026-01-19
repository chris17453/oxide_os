//! ICMP Protocol Implementation

use alloc::vec::Vec;

use crate::checksum;

/// ICMP type: Echo Reply
pub const ICMP_ECHO_REPLY: u8 = 0;

/// ICMP type: Destination Unreachable
pub const ICMP_DEST_UNREACHABLE: u8 = 3;

/// ICMP type: Source Quench
pub const ICMP_SOURCE_QUENCH: u8 = 4;

/// ICMP type: Redirect
pub const ICMP_REDIRECT: u8 = 5;

/// ICMP type: Echo Request
pub const ICMP_ECHO_REQUEST: u8 = 8;

/// ICMP type: Time Exceeded
pub const ICMP_TIME_EXCEEDED: u8 = 11;

/// ICMP type: Parameter Problem
pub const ICMP_PARAM_PROBLEM: u8 = 12;

/// ICMP type: Timestamp Request
pub const ICMP_TIMESTAMP_REQUEST: u8 = 13;

/// ICMP type: Timestamp Reply
pub const ICMP_TIMESTAMP_REPLY: u8 = 14;

/// ICMP header minimum length
pub const ICMP_HEADER_LEN: usize = 8;

/// ICMP destination unreachable codes
pub mod dest_unreachable {
    pub const NET_UNREACHABLE: u8 = 0;
    pub const HOST_UNREACHABLE: u8 = 1;
    pub const PROTOCOL_UNREACHABLE: u8 = 2;
    pub const PORT_UNREACHABLE: u8 = 3;
    pub const FRAGMENTATION_NEEDED: u8 = 4;
    pub const SOURCE_ROUTE_FAILED: u8 = 5;
}

/// ICMP packet
#[derive(Debug, Clone)]
pub struct IcmpPacket {
    /// Type
    pub icmp_type: u8,
    /// Code
    pub code: u8,
    /// Checksum
    pub checksum: u16,
    /// Identifier (for echo)
    pub identifier: u16,
    /// Sequence number (for echo)
    pub sequence: u16,
    /// Data
    pub data: Vec<u8>,
}

impl IcmpPacket {
    /// Create echo request
    pub fn new_echo_request(identifier: u16, sequence: u16, data: &[u8]) -> Self {
        IcmpPacket {
            icmp_type: ICMP_ECHO_REQUEST,
            code: 0,
            checksum: 0,
            identifier,
            sequence,
            data: data.to_vec(),
        }
    }

    /// Create echo reply
    pub fn new_echo_reply(identifier: u16, sequence: u16, data: &[u8]) -> Self {
        IcmpPacket {
            icmp_type: ICMP_ECHO_REPLY,
            code: 0,
            checksum: 0,
            identifier,
            sequence,
            data: data.to_vec(),
        }
    }

    /// Create destination unreachable
    pub fn new_dest_unreachable(code: u8, original_data: &[u8]) -> Self {
        // Include IP header + 8 bytes of original datagram
        let data_len = original_data.len().min(28 + 8);
        IcmpPacket {
            icmp_type: ICMP_DEST_UNREACHABLE,
            code,
            checksum: 0,
            identifier: 0,
            sequence: 0,
            data: original_data[..data_len].to_vec(),
        }
    }

    /// Parse ICMP packet
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < ICMP_HEADER_LEN {
            return None;
        }

        let icmp_type = data[0];
        let code = data[1];
        let checksum = u16::from_be_bytes([data[2], data[3]]);
        let identifier = u16::from_be_bytes([data[4], data[5]]);
        let sequence = u16::from_be_bytes([data[6], data[7]]);
        let packet_data = data[8..].to_vec();

        // Verify checksum
        let computed = checksum::internet_checksum(data);
        if computed != 0 {
            return None;
        }

        Some(IcmpPacket {
            icmp_type,
            code,
            checksum,
            identifier,
            sequence,
            data: packet_data,
        })
    }

    /// Serialize to bytes with computed checksum
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(ICMP_HEADER_LEN + self.data.len());

        buf.push(self.icmp_type);
        buf.push(self.code);
        // Checksum placeholder
        buf.extend_from_slice(&[0, 0]);
        buf.extend_from_slice(&self.identifier.to_be_bytes());
        buf.extend_from_slice(&self.sequence.to_be_bytes());
        buf.extend_from_slice(&self.data);

        // Compute checksum
        let checksum = checksum::internet_checksum(&buf);
        buf[2] = (checksum >> 8) as u8;
        buf[3] = checksum as u8;

        buf
    }
}
