//! IPv4 Protocol Implementation

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU16, Ordering};

use net::{Ipv4Addr, NetError, NetResult};

use crate::checksum;

/// IPv4 header minimum length
pub const IPV4_HEADER_MIN_LEN: usize = 20;

/// IP protocol numbers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum IpProtocol {
    /// ICMP
    Icmp = 1,
    /// TCP
    Tcp = 6,
    /// UDP
    Udp = 17,
    /// Unknown
    Unknown(u8),
}

impl From<u8> for IpProtocol {
    fn from(value: u8) -> Self {
        match value {
            1 => IpProtocol::Icmp,
            6 => IpProtocol::Tcp,
            17 => IpProtocol::Udp,
            other => IpProtocol::Unknown(other),
        }
    }
}

impl From<IpProtocol> for u8 {
    fn from(p: IpProtocol) -> u8 {
        match p {
            IpProtocol::Icmp => 1,
            IpProtocol::Tcp => 6,
            IpProtocol::Udp => 17,
            IpProtocol::Unknown(v) => v,
        }
    }
}

/// IPv4 header
#[derive(Debug, Clone, Copy)]
pub struct Ipv4Header {
    /// Version (4) and IHL
    pub version_ihl: u8,
    /// Type of Service
    pub tos: u8,
    /// Total length
    pub total_length: u16,
    /// Identification
    pub identification: u16,
    /// Flags and fragment offset
    pub flags_fragment: u16,
    /// Time to Live
    pub ttl: u8,
    /// Protocol
    pub protocol: IpProtocol,
    /// Header checksum
    pub checksum: u16,
    /// Source address
    pub src: Ipv4Addr,
    /// Destination address
    pub dst: Ipv4Addr,
}

impl Ipv4Header {
    /// Default TTL
    pub const DEFAULT_TTL: u8 = 64;

    /// Parse IPv4 header from bytes
    pub fn parse(data: &[u8]) -> NetResult<Self> {
        if data.len() < IPV4_HEADER_MIN_LEN {
            return Err(NetError::InvalidArgument);
        }

        let version_ihl = data[0];
        let version = version_ihl >> 4;
        if version != 4 {
            return Err(NetError::InvalidArgument);
        }

        let tos = data[1];
        let total_length = u16::from_be_bytes([data[2], data[3]]);
        let identification = u16::from_be_bytes([data[4], data[5]]);
        let flags_fragment = u16::from_be_bytes([data[6], data[7]]);
        let ttl = data[8];
        let protocol = IpProtocol::from(data[9]);
        let checksum = u16::from_be_bytes([data[10], data[11]]);
        let src = Ipv4Addr([data[12], data[13], data[14], data[15]]);
        let dst = Ipv4Addr([data[16], data[17], data[18], data[19]]);

        // Verify checksum
        let ihl = (version_ihl & 0x0F) as usize;
        let header_len = ihl * 4;
        if header_len > data.len() {
            return Err(NetError::InvalidArgument);
        }

        let computed_checksum = checksum::internet_checksum(&data[..header_len]);
        if computed_checksum != 0 {
            return Err(NetError::InvalidArgument);
        }

        Ok(Ipv4Header {
            version_ihl,
            tos,
            total_length,
            identification,
            flags_fragment,
            ttl,
            protocol,
            checksum,
            src,
            dst,
        })
    }

    /// Get header length in bytes
    pub fn header_len(&self) -> usize {
        ((self.version_ihl & 0x0F) as usize) * 4
    }

    /// Get payload length
    pub fn payload_len(&self) -> usize {
        (self.total_length as usize).saturating_sub(self.header_len())
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(IPV4_HEADER_MIN_LEN);

        buf.push(self.version_ihl);
        buf.push(self.tos);
        buf.extend_from_slice(&self.total_length.to_be_bytes());
        buf.extend_from_slice(&self.identification.to_be_bytes());
        buf.extend_from_slice(&self.flags_fragment.to_be_bytes());
        buf.push(self.ttl);
        buf.push(self.protocol.into());
        buf.extend_from_slice(&self.checksum.to_be_bytes());
        buf.extend_from_slice(&self.src.0);
        buf.extend_from_slice(&self.dst.0);

        buf
    }
}

/// IP identification counter
static IP_IDENTIFICATION: AtomicU16 = AtomicU16::new(1);

/// IPv4 packet
pub struct Ipv4Packet {
    /// Header
    pub header: Ipv4Header,
    /// Payload
    pub payload: Vec<u8>,
}

impl Ipv4Packet {
    /// Create a new IPv4 packet
    pub fn new(src: Ipv4Addr, dst: Ipv4Addr, protocol: IpProtocol, payload: &[u8]) -> Self {
        let total_length = (IPV4_HEADER_MIN_LEN + payload.len()) as u16;
        let identification = IP_IDENTIFICATION.fetch_add(1, Ordering::SeqCst);

        let header = Ipv4Header {
            version_ihl: 0x45, // Version 4, IHL 5 (20 bytes)
            tos: 0,
            total_length,
            identification,
            flags_fragment: 0x4000, // Don't fragment
            ttl: Ipv4Header::DEFAULT_TTL,
            protocol,
            checksum: 0,
            src,
            dst,
        };

        Ipv4Packet {
            header,
            payload: payload.to_vec(),
        }
    }

    /// Serialize to bytes with computed checksum
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(IPV4_HEADER_MIN_LEN + self.payload.len());

        // Build header bytes
        buf.push(self.header.version_ihl);
        buf.push(self.header.tos);
        buf.extend_from_slice(&self.header.total_length.to_be_bytes());
        buf.extend_from_slice(&self.header.identification.to_be_bytes());
        buf.extend_from_slice(&self.header.flags_fragment.to_be_bytes());
        buf.push(self.header.ttl);
        buf.push(self.header.protocol.into());
        // Checksum placeholder (will compute)
        buf.extend_from_slice(&[0, 0]);
        buf.extend_from_slice(&self.header.src.0);
        buf.extend_from_slice(&self.header.dst.0);

        // Compute checksum
        let checksum = checksum::internet_checksum(&buf);
        buf[10] = (checksum >> 8) as u8;
        buf[11] = checksum as u8;

        // Add payload
        buf.extend_from_slice(&self.payload);

        buf
    }
}

/// Pseudo header for TCP/UDP checksum calculation
#[derive(Debug, Clone, Copy)]
pub struct PseudoHeader {
    /// Source address
    pub src: Ipv4Addr,
    /// Destination address
    pub dst: Ipv4Addr,
    /// Zero
    pub zero: u8,
    /// Protocol
    pub protocol: u8,
    /// TCP/UDP length
    pub length: u16,
}

impl PseudoHeader {
    /// Create a new pseudo header
    pub fn new(src: Ipv4Addr, dst: Ipv4Addr, protocol: IpProtocol, length: u16) -> Self {
        PseudoHeader {
            src,
            dst,
            zero: 0,
            protocol: protocol.into(),
            length,
        }
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> [u8; 12] {
        let mut buf = [0u8; 12];
        buf[0..4].copy_from_slice(&self.src.0);
        buf[4..8].copy_from_slice(&self.dst.0);
        buf[8] = self.zero;
        buf[9] = self.protocol;
        buf[10..12].copy_from_slice(&self.length.to_be_bytes());
        buf
    }
}
