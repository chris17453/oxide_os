//! DNS Resolver Implementation
//!
//! Implements DNS client (RFC 1035).

#![no_std]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU16, Ordering};
use spin::Mutex;

use efflux_net::{IpAddr, Ipv4Addr, Ipv6Addr, NetError, NetResult};

/// DNS port
pub const DNS_PORT: u16 = 53;

/// Maximum DNS packet size (UDP)
pub const DNS_MAX_UDP_SIZE: usize = 512;

/// DNS class values
pub mod class {
    pub const IN: u16 = 1;  // Internet
    pub const CS: u16 = 2;  // CSNET
    pub const CH: u16 = 3;  // CHAOS
    pub const HS: u16 = 4;  // Hesiod
}

/// DNS record types
pub mod record_type {
    pub const A: u16 = 1;      // IPv4 address
    pub const NS: u16 = 2;     // Name server
    pub const CNAME: u16 = 5;  // Canonical name
    pub const SOA: u16 = 6;    // Start of authority
    pub const PTR: u16 = 12;   // Pointer
    pub const MX: u16 = 15;    // Mail exchange
    pub const TXT: u16 = 16;   // Text
    pub const AAAA: u16 = 28;  // IPv6 address
    pub const SRV: u16 = 33;   // Service
    pub const ANY: u16 = 255;  // Any
}

/// DNS response codes
pub mod rcode {
    pub const NOERROR: u8 = 0;   // No error
    pub const FORMERR: u8 = 1;   // Format error
    pub const SERVFAIL: u8 = 2;  // Server failure
    pub const NXDOMAIN: u8 = 3;  // Non-existent domain
    pub const NOTIMP: u8 = 4;    // Not implemented
    pub const REFUSED: u8 = 5;   // Query refused
}

/// DNS header flags
pub mod flags {
    pub const QR: u16 = 0x8000;       // Query/Response
    pub const OPCODE: u16 = 0x7800;   // Opcode
    pub const AA: u16 = 0x0400;       // Authoritative answer
    pub const TC: u16 = 0x0200;       // Truncated
    pub const RD: u16 = 0x0100;       // Recursion desired
    pub const RA: u16 = 0x0080;       // Recursion available
    pub const RCODE: u16 = 0x000F;    // Response code
}

/// DNS header
#[derive(Debug, Clone, Copy)]
pub struct DnsHeader {
    /// Transaction ID
    pub id: u16,
    /// Flags
    pub flags: u16,
    /// Question count
    pub qdcount: u16,
    /// Answer count
    pub ancount: u16,
    /// Authority count
    pub nscount: u16,
    /// Additional count
    pub arcount: u16,
}

impl DnsHeader {
    /// Header length
    pub const LEN: usize = 12;

    /// Parse header from bytes
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < Self::LEN {
            return None;
        }

        Some(DnsHeader {
            id: u16::from_be_bytes([data[0], data[1]]),
            flags: u16::from_be_bytes([data[2], data[3]]),
            qdcount: u16::from_be_bytes([data[4], data[5]]),
            ancount: u16::from_be_bytes([data[6], data[7]]),
            nscount: u16::from_be_bytes([data[8], data[9]]),
            arcount: u16::from_be_bytes([data[10], data[11]]),
        })
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> [u8; Self::LEN] {
        let mut buf = [0u8; Self::LEN];
        buf[0..2].copy_from_slice(&self.id.to_be_bytes());
        buf[2..4].copy_from_slice(&self.flags.to_be_bytes());
        buf[4..6].copy_from_slice(&self.qdcount.to_be_bytes());
        buf[6..8].copy_from_slice(&self.ancount.to_be_bytes());
        buf[8..10].copy_from_slice(&self.nscount.to_be_bytes());
        buf[10..12].copy_from_slice(&self.arcount.to_be_bytes());
        buf
    }

    /// Check if this is a response
    pub fn is_response(&self) -> bool {
        self.flags & flags::QR != 0
    }

    /// Get response code
    pub fn rcode(&self) -> u8 {
        (self.flags & flags::RCODE) as u8
    }
}

/// DNS question
#[derive(Debug, Clone)]
pub struct DnsQuestion {
    /// Name
    pub name: String,
    /// Type
    pub qtype: u16,
    /// Class
    pub qclass: u16,
}

impl DnsQuestion {
    /// Parse question from bytes, returns (question, bytes consumed)
    pub fn parse(data: &[u8], offset: usize) -> Option<(Self, usize)> {
        let (name, new_offset) = parse_name(data, offset)?;

        if new_offset + 4 > data.len() {
            return None;
        }

        let qtype = u16::from_be_bytes([data[new_offset], data[new_offset + 1]]);
        let qclass = u16::from_be_bytes([data[new_offset + 2], data[new_offset + 3]]);

        Some((
            DnsQuestion { name, qtype, qclass },
            new_offset + 4,
        ))
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = encode_name(&self.name);
        buf.extend_from_slice(&self.qtype.to_be_bytes());
        buf.extend_from_slice(&self.qclass.to_be_bytes());
        buf
    }
}

/// DNS resource record
#[derive(Debug, Clone)]
pub struct DnsRecord {
    /// Name
    pub name: String,
    /// Type
    pub rtype: u16,
    /// Class
    pub rclass: u16,
    /// TTL
    pub ttl: u32,
    /// Data
    pub rdata: Vec<u8>,
}

impl DnsRecord {
    /// Parse record from bytes, returns (record, bytes consumed)
    pub fn parse(data: &[u8], offset: usize) -> Option<(Self, usize)> {
        let (name, mut pos) = parse_name(data, offset)?;

        if pos + 10 > data.len() {
            return None;
        }

        let rtype = u16::from_be_bytes([data[pos], data[pos + 1]]);
        let rclass = u16::from_be_bytes([data[pos + 2], data[pos + 3]]);
        let ttl = u32::from_be_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]]);
        let rdlen = u16::from_be_bytes([data[pos + 8], data[pos + 9]]) as usize;
        pos += 10;

        if pos + rdlen > data.len() {
            return None;
        }

        let rdata = data[pos..pos + rdlen].to_vec();
        pos += rdlen;

        Some((
            DnsRecord {
                name,
                rtype,
                rclass,
                ttl,
                rdata,
            },
            pos,
        ))
    }

    /// Get IPv4 address from A record
    pub fn as_ipv4(&self) -> Option<Ipv4Addr> {
        if self.rtype == record_type::A && self.rdata.len() == 4 {
            Some(Ipv4Addr([self.rdata[0], self.rdata[1], self.rdata[2], self.rdata[3]]))
        } else {
            None
        }
    }

    /// Get IPv6 address from AAAA record
    pub fn as_ipv6(&self) -> Option<Ipv6Addr> {
        if self.rtype == record_type::AAAA && self.rdata.len() == 16 {
            let mut addr = [0u8; 16];
            addr.copy_from_slice(&self.rdata);
            Some(Ipv6Addr(addr))
        } else {
            None
        }
    }
}

/// DNS message
#[derive(Debug, Clone)]
pub struct DnsMessage {
    /// Header
    pub header: DnsHeader,
    /// Questions
    pub questions: Vec<DnsQuestion>,
    /// Answers
    pub answers: Vec<DnsRecord>,
    /// Authority records
    pub authority: Vec<DnsRecord>,
    /// Additional records
    pub additional: Vec<DnsRecord>,
}

impl DnsMessage {
    /// Create a new query message
    pub fn new_query(id: u16, name: &str, qtype: u16) -> Self {
        DnsMessage {
            header: DnsHeader {
                id,
                flags: flags::RD, // Recursion desired
                qdcount: 1,
                ancount: 0,
                nscount: 0,
                arcount: 0,
            },
            questions: vec![DnsQuestion {
                name: String::from(name),
                qtype,
                qclass: class::IN,
            }],
            answers: Vec::new(),
            authority: Vec::new(),
            additional: Vec::new(),
        }
    }

    /// Parse message from bytes
    pub fn parse(data: &[u8]) -> Option<Self> {
        let header = DnsHeader::parse(data)?;
        let mut offset = DnsHeader::LEN;

        // Parse questions
        let mut questions = Vec::new();
        for _ in 0..header.qdcount {
            let (q, new_offset) = DnsQuestion::parse(data, offset)?;
            questions.push(q);
            offset = new_offset;
        }

        // Parse answers
        let mut answers = Vec::new();
        for _ in 0..header.ancount {
            let (r, new_offset) = DnsRecord::parse(data, offset)?;
            answers.push(r);
            offset = new_offset;
        }

        // Parse authority
        let mut authority = Vec::new();
        for _ in 0..header.nscount {
            let (r, new_offset) = DnsRecord::parse(data, offset)?;
            authority.push(r);
            offset = new_offset;
        }

        // Parse additional
        let mut additional = Vec::new();
        for _ in 0..header.arcount {
            let (r, new_offset) = DnsRecord::parse(data, offset)?;
            additional.push(r);
            offset = new_offset;
        }

        Some(DnsMessage {
            header,
            questions,
            answers,
            authority,
            additional,
        })
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        // Header
        buf.extend_from_slice(&self.header.to_bytes());

        // Questions
        for q in &self.questions {
            buf.extend_from_slice(&q.to_bytes());
        }

        // Records would be serialized here for responses

        buf
    }
}

/// Parse a domain name from DNS message
fn parse_name(data: &[u8], mut offset: usize) -> Option<(String, usize)> {
    let mut name = String::new();
    let mut jumped = false;
    let mut end_offset = offset;
    let mut jumps = 0;

    loop {
        if offset >= data.len() {
            return None;
        }

        let len = data[offset];

        // Check for compression pointer
        if len & 0xC0 == 0xC0 {
            if offset + 1 >= data.len() {
                return None;
            }

            let ptr = (((len & 0x3F) as usize) << 8) | (data[offset + 1] as usize);

            if !jumped {
                end_offset = offset + 2;
            }

            offset = ptr;
            jumped = true;
            jumps += 1;

            // Prevent infinite loops
            if jumps > 10 {
                return None;
            }

            continue;
        }

        if len == 0 {
            if !jumped {
                end_offset = offset + 1;
            }
            break;
        }

        offset += 1;
        if offset + (len as usize) > data.len() {
            return None;
        }

        if !name.is_empty() {
            name.push('.');
        }

        for i in 0..(len as usize) {
            name.push(data[offset + i] as char);
        }

        offset += len as usize;
    }

    Some((name, end_offset))
}

/// Encode a domain name
fn encode_name(name: &str) -> Vec<u8> {
    let mut buf = Vec::new();

    for label in name.split('.') {
        if label.is_empty() {
            continue;
        }
        buf.push(label.len() as u8);
        buf.extend_from_slice(label.as_bytes());
    }

    buf.push(0); // Root label
    buf
}

/// DNS cache entry
#[derive(Debug, Clone)]
pub struct CacheEntry {
    /// Records
    pub records: Vec<DnsRecord>,
    /// Expiry timestamp
    pub expires: u64,
}

/// DNS resolver
pub struct DnsResolver {
    /// DNS servers
    servers: Mutex<Vec<Ipv4Addr>>,
    /// Cache
    cache: Mutex<BTreeMap<String, CacheEntry>>,
    /// Next query ID
    next_id: AtomicU16,
}

impl DnsResolver {
    /// Create a new DNS resolver
    pub fn new() -> Self {
        DnsResolver {
            servers: Mutex::new(Vec::new()),
            cache: Mutex::new(BTreeMap::new()),
            next_id: AtomicU16::new(1),
        }
    }

    /// Add a DNS server
    pub fn add_server(&self, server: Ipv4Addr) {
        self.servers.lock().push(server);
    }

    /// Set DNS servers
    pub fn set_servers(&self, servers: Vec<Ipv4Addr>) {
        *self.servers.lock() = servers;
    }

    /// Get DNS servers
    pub fn servers(&self) -> Vec<Ipv4Addr> {
        self.servers.lock().clone()
    }

    /// Create a query packet for A record
    pub fn create_query(&self, name: &str) -> (u16, Vec<u8>) {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let msg = DnsMessage::new_query(id, name, record_type::A);
        (id, msg.to_bytes())
    }

    /// Create a query packet for AAAA record
    pub fn create_query_aaaa(&self, name: &str) -> (u16, Vec<u8>) {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let msg = DnsMessage::new_query(id, name, record_type::AAAA);
        (id, msg.to_bytes())
    }

    /// Process a DNS response
    pub fn process_response(&self, data: &[u8], expected_id: u16) -> NetResult<Vec<IpAddr>> {
        let msg = DnsMessage::parse(data).ok_or(NetError::InvalidArgument)?;

        // Verify ID
        if msg.header.id != expected_id {
            return Err(NetError::InvalidArgument);
        }

        // Check for errors
        let rcode = msg.header.rcode();
        if rcode != rcode::NOERROR {
            return match rcode {
                rcode::NXDOMAIN => Err(NetError::HostUnreachable),
                rcode::SERVFAIL => Err(NetError::Timeout),
                _ => Err(NetError::InvalidArgument),
            };
        }

        // Extract addresses
        let mut addrs = Vec::new();
        for record in &msg.answers {
            if let Some(ipv4) = record.as_ipv4() {
                addrs.push(IpAddr::V4(ipv4));
            }
            if let Some(ipv6) = record.as_ipv6() {
                addrs.push(IpAddr::V6(ipv6));
            }
        }

        // Cache results
        if !msg.questions.is_empty() && !msg.answers.is_empty() {
            let name = &msg.questions[0].name;
            let min_ttl = msg.answers.iter().map(|r| r.ttl).min().unwrap_or(300);

            self.cache.lock().insert(
                name.clone(),
                CacheEntry {
                    records: msg.answers.clone(),
                    expires: min_ttl as u64, // Would use actual timestamp
                },
            );
        }

        Ok(addrs)
    }

    /// Lookup from cache
    pub fn lookup_cache(&self, name: &str) -> Option<Vec<IpAddr>> {
        let cache = self.cache.lock();
        if let Some(entry) = cache.get(name) {
            // Would check expiry here
            let mut addrs = Vec::new();
            for record in &entry.records {
                if let Some(ipv4) = record.as_ipv4() {
                    addrs.push(IpAddr::V4(ipv4));
                }
                if let Some(ipv6) = record.as_ipv6() {
                    addrs.push(IpAddr::V6(ipv6));
                }
            }
            if !addrs.is_empty() {
                return Some(addrs);
            }
        }
        None
    }

    /// Clear cache
    pub fn clear_cache(&self) {
        self.cache.lock().clear();
    }
}

impl Default for DnsResolver {
    fn default() -> Self {
        Self::new()
    }
}

/// Global DNS resolver
static RESOLVER: Mutex<Option<Arc<DnsResolver>>> = Mutex::new(None);

/// Initialize the DNS resolver
pub fn init() {
    let resolver = Arc::new(DnsResolver::new());
    *RESOLVER.lock() = Some(resolver);
}

/// Get the DNS resolver
pub fn resolver() -> Option<Arc<DnsResolver>> {
    RESOLVER.lock().clone()
}
