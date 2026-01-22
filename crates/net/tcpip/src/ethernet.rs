//! Ethernet Frame Handling

use alloc::vec;
use alloc::vec::Vec;
use net::{MacAddress, NetError, NetResult};

/// Ethernet header length
pub const ETHERNET_HEADER_LEN: usize = 14;

/// Minimum Ethernet frame size (without FCS)
pub const ETHERNET_MIN_LEN: usize = 60;

/// Maximum Ethernet frame size (without FCS)
pub const ETHERNET_MAX_LEN: usize = 1514;

/// Ethernet type values
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum EtherType {
    /// IPv4
    Ipv4 = 0x0800,
    /// ARP
    Arp = 0x0806,
    /// IPv6
    Ipv6 = 0x86DD,
    /// VLAN tag
    Vlan = 0x8100,
    /// Unknown
    Unknown(u16),
}

impl From<u16> for EtherType {
    fn from(value: u16) -> Self {
        match value {
            0x0800 => EtherType::Ipv4,
            0x0806 => EtherType::Arp,
            0x86DD => EtherType::Ipv6,
            0x8100 => EtherType::Vlan,
            other => EtherType::Unknown(other),
        }
    }
}

impl From<EtherType> for u16 {
    fn from(et: EtherType) -> u16 {
        match et {
            EtherType::Ipv4 => 0x0800,
            EtherType::Arp => 0x0806,
            EtherType::Ipv6 => 0x86DD,
            EtherType::Vlan => 0x8100,
            EtherType::Unknown(v) => v,
        }
    }
}

/// Ethernet header
#[derive(Debug, Clone, Copy)]
pub struct EthernetHeader {
    /// Destination MAC address
    pub dst: MacAddress,
    /// Source MAC address
    pub src: MacAddress,
    /// EtherType
    pub ethertype: EtherType,
}

impl EthernetHeader {
    /// Parse Ethernet header from bytes
    pub fn parse(data: &[u8]) -> NetResult<Self> {
        if data.len() < ETHERNET_HEADER_LEN {
            return Err(NetError::InvalidArgument);
        }

        let dst = MacAddress([data[0], data[1], data[2], data[3], data[4], data[5]]);
        let src = MacAddress([data[6], data[7], data[8], data[9], data[10], data[11]]);
        let ethertype = EtherType::from(u16::from_be_bytes([data[12], data[13]]));

        Ok(EthernetHeader {
            dst,
            src,
            ethertype,
        })
    }

    /// Serialize header to bytes
    pub fn to_bytes(&self) -> [u8; ETHERNET_HEADER_LEN] {
        let mut buf = [0u8; ETHERNET_HEADER_LEN];
        buf[0..6].copy_from_slice(&self.dst.0);
        buf[6..12].copy_from_slice(&self.src.0);
        let et: u16 = self.ethertype.into();
        buf[12..14].copy_from_slice(&et.to_be_bytes());
        buf
    }
}

/// Ethernet frame
pub struct EthernetFrame {
    /// Header
    pub header: EthernetHeader,
    /// Payload
    pub payload: Vec<u8>,
}

impl EthernetFrame {
    /// Create a new Ethernet frame
    pub fn new(dst: MacAddress, src: MacAddress, ethertype: EtherType, payload: &[u8]) -> Self {
        EthernetFrame {
            header: EthernetHeader {
                dst,
                src,
                ethertype,
            },
            payload: payload.to_vec(),
        }
    }

    /// Parse frame from bytes
    pub fn parse(data: &[u8]) -> NetResult<Self> {
        let header = EthernetHeader::parse(data)?;
        let payload = data[ETHERNET_HEADER_LEN..].to_vec();
        Ok(EthernetFrame { header, payload })
    }

    /// Serialize frame to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = vec![0u8; ETHERNET_HEADER_LEN + self.payload.len()];
        buf[..ETHERNET_HEADER_LEN].copy_from_slice(&self.header.to_bytes());
        buf[ETHERNET_HEADER_LEN..].copy_from_slice(&self.payload);

        // Pad to minimum size if needed
        if buf.len() < ETHERNET_MIN_LEN {
            buf.resize(ETHERNET_MIN_LEN, 0);
        }

        buf
    }
}
