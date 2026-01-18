//! ARP Protocol Implementation

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use spin::Mutex;

use efflux_net::{Ipv4Addr, MacAddress};

/// ARP hardware type: Ethernet
pub const ARP_HTYPE_ETHERNET: u16 = 1;

/// ARP protocol type: IPv4
pub const ARP_PTYPE_IPV4: u16 = 0x0800;

/// ARP operation: Request
pub const ARP_REQUEST: u16 = 1;

/// ARP operation: Reply
pub const ARP_REPLY: u16 = 2;

/// ARP packet length
pub const ARP_PACKET_LEN: usize = 28;

/// ARP packet
#[derive(Debug, Clone, Copy)]
pub struct ArpPacket {
    /// Hardware type
    pub htype: u16,
    /// Protocol type
    pub ptype: u16,
    /// Hardware address length
    pub hlen: u8,
    /// Protocol address length
    pub plen: u8,
    /// Operation
    pub operation: u16,
    /// Sender hardware address
    pub sender_mac: MacAddress,
    /// Sender protocol address
    pub sender_ip: Ipv4Addr,
    /// Target hardware address
    pub target_mac: MacAddress,
    /// Target protocol address
    pub target_ip: Ipv4Addr,
}

impl ArpPacket {
    /// Create new ARP request
    pub fn new_request(sender_mac: MacAddress, sender_ip: Ipv4Addr, target_ip: Ipv4Addr) -> Self {
        ArpPacket {
            htype: ARP_HTYPE_ETHERNET,
            ptype: ARP_PTYPE_IPV4,
            hlen: 6,
            plen: 4,
            operation: ARP_REQUEST,
            sender_mac,
            sender_ip,
            target_mac: MacAddress([0; 6]),
            target_ip,
        }
    }

    /// Create new ARP reply
    pub fn new_reply(
        sender_mac: MacAddress,
        sender_ip: Ipv4Addr,
        target_mac: MacAddress,
        target_ip: Ipv4Addr,
    ) -> Self {
        ArpPacket {
            htype: ARP_HTYPE_ETHERNET,
            ptype: ARP_PTYPE_IPV4,
            hlen: 6,
            plen: 4,
            operation: ARP_REPLY,
            sender_mac,
            sender_ip,
            target_mac,
            target_ip,
        }
    }

    /// Parse ARP packet from bytes
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < ARP_PACKET_LEN {
            return None;
        }

        let htype = u16::from_be_bytes([data[0], data[1]]);
        let ptype = u16::from_be_bytes([data[2], data[3]]);
        let hlen = data[4];
        let plen = data[5];
        let operation = u16::from_be_bytes([data[6], data[7]]);

        // Validate for Ethernet + IPv4
        if htype != ARP_HTYPE_ETHERNET || ptype != ARP_PTYPE_IPV4 || hlen != 6 || plen != 4 {
            return None;
        }

        let sender_mac = MacAddress([data[8], data[9], data[10], data[11], data[12], data[13]]);
        let sender_ip = Ipv4Addr([data[14], data[15], data[16], data[17]]);
        let target_mac = MacAddress([data[18], data[19], data[20], data[21], data[22], data[23]]);
        let target_ip = Ipv4Addr([data[24], data[25], data[26], data[27]]);

        Some(ArpPacket {
            htype,
            ptype,
            hlen,
            plen,
            operation,
            sender_mac,
            sender_ip,
            target_mac,
            target_ip,
        })
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(ARP_PACKET_LEN);

        buf.extend_from_slice(&self.htype.to_be_bytes());
        buf.extend_from_slice(&self.ptype.to_be_bytes());
        buf.push(self.hlen);
        buf.push(self.plen);
        buf.extend_from_slice(&self.operation.to_be_bytes());
        buf.extend_from_slice(&self.sender_mac.0);
        buf.extend_from_slice(&self.sender_ip.0);
        buf.extend_from_slice(&self.target_mac.0);
        buf.extend_from_slice(&self.target_ip.0);

        buf
    }
}

/// ARP cache entry
#[derive(Debug, Clone, Copy)]
pub struct ArpCacheEntry {
    /// MAC address
    pub mac: MacAddress,
    /// Timestamp (in some unit, e.g., ticks)
    pub timestamp: u64,
}

/// ARP cache timeout (arbitrary units for now)
const ARP_CACHE_TIMEOUT: u64 = 300;

/// ARP cache
pub struct ArpCache {
    /// Cache entries
    entries: Mutex<BTreeMap<u32, ArpCacheEntry>>,
}

impl ArpCache {
    /// Create a new ARP cache
    pub fn new() -> Self {
        ArpCache {
            entries: Mutex::new(BTreeMap::new()),
        }
    }

    /// Insert an entry
    pub fn insert(&self, ip: Ipv4Addr, mac: MacAddress) {
        let key = ip.to_u32();
        let entry = ArpCacheEntry {
            mac,
            timestamp: 0, // Would use actual timestamp
        };
        self.entries.lock().insert(key, entry);
    }

    /// Lookup an entry
    pub fn lookup(&self, ip: Ipv4Addr) -> Option<MacAddress> {
        let key = ip.to_u32();
        self.entries.lock().get(&key).map(|e| e.mac)
    }

    /// Remove an entry
    pub fn remove(&self, ip: Ipv4Addr) {
        let key = ip.to_u32();
        self.entries.lock().remove(&key);
    }

    /// Clear expired entries
    pub fn expire(&self, current_time: u64) {
        let mut entries = self.entries.lock();
        entries.retain(|_, entry| current_time - entry.timestamp < ARP_CACHE_TIMEOUT);
    }

    /// Clear all entries
    pub fn clear(&self) {
        self.entries.lock().clear();
    }
}

impl Default for ArpCache {
    fn default() -> Self {
        Self::new()
    }
}
