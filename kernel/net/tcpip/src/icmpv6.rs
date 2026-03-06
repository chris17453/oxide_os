//! ICMPv6 Implementation
//!
//! — ShadePacket: Minimum viable ICMPv6 — echo request/reply for ping6.
//! No NDP (Neighbor Discovery) yet — that's the ARP equivalent for IPv6.
//! Uses IPv6 pseudo-header checksum per RFC 4443.

use alloc::vec::Vec;
use net::{Ipv6Addr, NetResult};

use crate::checksum;

/// ICMPv6 type: Echo Request
pub const ICMPV6_ECHO_REQUEST: u8 = 128;
/// ICMPv6 type: Echo Reply
pub const ICMPV6_ECHO_REPLY: u8 = 129;

/// ICMPv6 packet
#[derive(Debug, Clone)]
pub struct Icmpv6Packet {
    /// ICMPv6 type
    pub icmp_type: u8,
    /// ICMPv6 code
    pub code: u8,
    /// Identifier (for echo)
    pub identifier: u16,
    /// Sequence number (for echo)
    pub sequence: u16,
    /// Payload data
    pub data: Vec<u8>,
}

impl Icmpv6Packet {
    /// Parse ICMPv6 packet from bytes
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 8 {
            return None;
        }

        let icmp_type = data[0];
        let code = data[1];
        // bytes 2-3: checksum (verified separately)
        let identifier = u16::from_be_bytes([data[4], data[5]]);
        let sequence = u16::from_be_bytes([data[6], data[7]]);

        Some(Icmpv6Packet {
            icmp_type,
            code,
            identifier,
            sequence,
            data: data[8..].to_vec(),
        })
    }

    /// Create an echo reply
    pub fn new_echo_reply(id: u16, seq: u16, data: &[u8]) -> Self {
        Icmpv6Packet {
            icmp_type: ICMPV6_ECHO_REPLY,
            code: 0,
            identifier: id,
            sequence: seq,
            data: data.to_vec(),
        }
    }

    /// Create an echo request
    pub fn new_echo_request(id: u16, seq: u16, data: &[u8]) -> Self {
        Icmpv6Packet {
            icmp_type: ICMPV6_ECHO_REQUEST,
            code: 0,
            identifier: id,
            sequence: seq,
            data: data.to_vec(),
        }
    }

    /// Serialize to bytes with checksum computed using IPv6 pseudo-header
    /// — ShadePacket: ICMPv6 checksum includes pseudo-header per RFC 4443 §2.3
    pub fn to_bytes_with_checksum(&self, src: Ipv6Addr, dst: Ipv6Addr) -> Vec<u8> {
        let mut buf = Vec::with_capacity(8 + self.data.len());

        buf.push(self.icmp_type);
        buf.push(self.code);
        buf.push(0); // checksum placeholder
        buf.push(0);
        buf.extend_from_slice(&self.identifier.to_be_bytes());
        buf.extend_from_slice(&self.sequence.to_be_bytes());
        buf.extend_from_slice(&self.data);

        // — ShadePacket: IPv6 pseudo-header for ICMPv6 checksum
        // src(16) + dst(16) + upper-layer-length(4) + zero(3) + next-header(1) = 40 bytes
        let mut pseudo = Vec::with_capacity(40);
        pseudo.extend_from_slice(&src.0);
        pseudo.extend_from_slice(&dst.0);
        pseudo.extend_from_slice(&(buf.len() as u32).to_be_bytes());
        pseudo.extend_from_slice(&[0, 0, 0, 58]); // next header = ICMPv6

        let cksum = checksum::checksum_with_pseudo(&pseudo, &buf);
        buf[2] = (cksum >> 8) as u8;
        buf[3] = cksum as u8;

        buf
    }
}
