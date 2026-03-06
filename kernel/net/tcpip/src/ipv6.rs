//! IPv6 Protocol Implementation
//!
//! — ShadePacket: Minimum viable IPv6 — parse headers, send packets, loopback.
//! No extension headers, no fragmentation, no NDP yet. Just enough to
//! make ping6 ::1 work and stop silently dropping v6 traffic.

use alloc::vec::Vec;
use net::{Ipv6Addr, NetError, NetResult};

/// IPv6 header length (fixed, unlike IPv4)
pub const IPV6_HEADER_LEN: usize = 40;

/// IPv6 next header values (same as IPv4 protocol numbers for transport)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum NextHeader {
    /// ICMPv6
    Icmpv6 = 58,
    /// TCP
    Tcp = 6,
    /// UDP
    Udp = 17,
    /// Unknown / extension header we don't handle
    Unknown(u8),
}

impl From<u8> for NextHeader {
    fn from(value: u8) -> Self {
        match value {
            58 => NextHeader::Icmpv6,
            6 => NextHeader::Tcp,
            17 => NextHeader::Udp,
            other => NextHeader::Unknown(other),
        }
    }
}

impl From<NextHeader> for u8 {
    fn from(nh: NextHeader) -> u8 {
        match nh {
            NextHeader::Icmpv6 => 58,
            NextHeader::Tcp => 6,
            NextHeader::Udp => 17,
            NextHeader::Unknown(v) => v,
        }
    }
}

/// IPv6 header
#[derive(Debug, Clone, Copy)]
pub struct Ipv6Header {
    /// Version (4 bits), Traffic Class (8 bits), Flow Label (20 bits)
    pub version_tc_flow: u32,
    /// Payload length (not including header)
    pub payload_length: u16,
    /// Next header (protocol)
    pub next_header: NextHeader,
    /// Hop limit (TTL equivalent)
    pub hop_limit: u8,
    /// Source address
    pub src: Ipv6Addr,
    /// Destination address
    pub dst: Ipv6Addr,
}

impl Ipv6Header {
    /// Default hop limit
    pub const DEFAULT_HOP_LIMIT: u8 = 64;

    /// Parse IPv6 header from bytes
    pub fn parse(data: &[u8]) -> NetResult<Self> {
        if data.len() < IPV6_HEADER_LEN {
            return Err(NetError::InvalidArgument);
        }

        let version_tc_flow = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        let version = (version_tc_flow >> 28) as u8;
        if version != 6 {
            return Err(NetError::InvalidArgument);
        }

        let payload_length = u16::from_be_bytes([data[4], data[5]]);
        let next_header = NextHeader::from(data[6]);
        let hop_limit = data[7];

        let mut src = [0u8; 16];
        let mut dst = [0u8; 16];
        src.copy_from_slice(&data[8..24]);
        dst.copy_from_slice(&data[24..40]);

        Ok(Ipv6Header {
            version_tc_flow,
            payload_length,
            next_header,
            hop_limit,
            src: Ipv6Addr(src),
            dst: Ipv6Addr(dst),
        })
    }
}

/// IPv6 packet
pub struct Ipv6Packet {
    /// Header
    pub header: Ipv6Header,
    /// Payload
    pub payload: Vec<u8>,
}

impl Ipv6Packet {
    /// Create a new IPv6 packet
    pub fn new(src: Ipv6Addr, dst: Ipv6Addr, next_header: NextHeader, payload: &[u8]) -> Self {
        let header = Ipv6Header {
            // — ShadePacket: version=6, traffic class=0, flow label=0
            version_tc_flow: 0x6000_0000,
            payload_length: payload.len() as u16,
            next_header,
            hop_limit: Ipv6Header::DEFAULT_HOP_LIMIT,
            src,
            dst,
        };

        Ipv6Packet {
            header,
            payload: payload.to_vec(),
        }
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(IPV6_HEADER_LEN + self.payload.len());

        buf.extend_from_slice(&self.header.version_tc_flow.to_be_bytes());
        buf.extend_from_slice(&self.header.payload_length.to_be_bytes());
        buf.push(self.header.next_header.into());
        buf.push(self.header.hop_limit);
        buf.extend_from_slice(&self.header.src.0);
        buf.extend_from_slice(&self.header.dst.0);
        buf.extend_from_slice(&self.payload);

        buf
    }
}
