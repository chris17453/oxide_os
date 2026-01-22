//! DNS resolution support
//!
//! Provides hostname resolution using DNS queries.

use crate::close;
use crate::socket::{
    SOCKADDR_IN_SIZE, SockAddrIn, af, htons, ipproto, recvfrom, sendto, sock, sockaddr_in_octets,
    socket,
};

/// Default DNS server (Google Public DNS)
pub const DEFAULT_DNS_SERVER: (u8, u8, u8, u8) = (8, 8, 8, 8);

/// DNS port
pub const DNS_PORT: u16 = 53;

/// Maximum hostname length
pub const MAX_HOSTNAME: usize = 256;

/// DNS record type A (IPv4)
pub const DNS_TYPE_A: u16 = 1;

/// DNS record type AAAA (IPv6)
pub const DNS_TYPE_AAAA: u16 = 28;

/// DNS class IN (Internet)
pub const DNS_CLASS_IN: u16 = 1;

/// DNS header flags
const DNS_RD: u16 = 0x0100; // Recursion Desired

/// Result of DNS resolution
#[derive(Clone, Copy)]
pub struct ResolvedAddr {
    /// IPv4 address (if resolved)
    pub ipv4: Option<(u8, u8, u8, u8)>,
    /// IPv6 address (if resolved)
    pub ipv6: Option<[u8; 16]>,
}

impl Default for ResolvedAddr {
    fn default() -> Self {
        Self {
            ipv4: None,
            ipv6: None,
        }
    }
}

/// Build DNS query packet
fn build_query(hostname: &str, qtype: u16, buf: &mut [u8]) -> usize {
    let mut pos = 0;

    // Transaction ID (use simple value, could be randomized)
    let id: u16 = 0x1234;
    buf[pos] = (id >> 8) as u8;
    buf[pos + 1] = (id & 0xFF) as u8;
    pos += 2;

    // Flags: standard query with recursion desired
    let flags = htons(DNS_RD);
    buf[pos] = (flags >> 8) as u8;
    buf[pos + 1] = (flags & 0xFF) as u8;
    pos += 2;

    // QDCOUNT = 1
    buf[pos] = 0;
    buf[pos + 1] = 1;
    pos += 2;

    // ANCOUNT = 0
    buf[pos] = 0;
    buf[pos + 1] = 0;
    pos += 2;

    // NSCOUNT = 0
    buf[pos] = 0;
    buf[pos + 1] = 0;
    pos += 2;

    // ARCOUNT = 0
    buf[pos] = 0;
    buf[pos + 1] = 0;
    pos += 2;

    // QNAME (domain name in label format)
    for label in hostname.split('.') {
        let label_bytes = label.as_bytes();
        if label_bytes.is_empty() || label_bytes.len() > 63 {
            continue;
        }
        buf[pos] = label_bytes.len() as u8;
        pos += 1;
        buf[pos..pos + label_bytes.len()].copy_from_slice(label_bytes);
        pos += label_bytes.len();
    }
    buf[pos] = 0; // End of QNAME
    pos += 1;

    // QTYPE
    buf[pos] = (qtype >> 8) as u8;
    buf[pos + 1] = (qtype & 0xFF) as u8;
    pos += 2;

    // QCLASS = IN
    buf[pos] = 0;
    buf[pos + 1] = 1;
    pos += 2;

    pos
}

/// Parse DNS response and extract IP addresses
fn parse_response(buf: &[u8], len: usize) -> ResolvedAddr {
    let mut result = ResolvedAddr::default();

    if len < 12 {
        return result;
    }

    // Get answer count
    let ancount = ((buf[6] as u16) << 8) | (buf[7] as u16);
    if ancount == 0 {
        return result;
    }

    // Skip header (12 bytes)
    let mut pos = 12;

    // Skip question section (QNAME + QTYPE + QCLASS)
    while pos < len && buf[pos] != 0 {
        if buf[pos] & 0xC0 == 0xC0 {
            // Compression pointer
            pos += 2;
            break;
        }
        pos += buf[pos] as usize + 1;
    }
    if pos < len && buf[pos] == 0 {
        pos += 1;
    }
    pos += 4; // QTYPE + QCLASS

    // Parse answer records
    for _ in 0..ancount {
        if pos + 12 > len {
            break;
        }

        // Skip NAME (might be compressed)
        if buf[pos] & 0xC0 == 0xC0 {
            pos += 2;
        } else {
            while pos < len && buf[pos] != 0 {
                pos += buf[pos] as usize + 1;
            }
            pos += 1;
        }

        if pos + 10 > len {
            break;
        }

        let rtype = ((buf[pos] as u16) << 8) | (buf[pos + 1] as u16);
        pos += 2;

        // Skip RCLASS
        pos += 2;

        // Skip TTL
        pos += 4;

        let rdlength = ((buf[pos] as u16) << 8) | (buf[pos + 1] as u16);
        pos += 2;

        if pos + rdlength as usize > len {
            break;
        }

        match rtype {
            DNS_TYPE_A if rdlength == 4 => {
                if result.ipv4.is_none() {
                    result.ipv4 = Some((buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]));
                }
            }
            DNS_TYPE_AAAA if rdlength == 16 => {
                if result.ipv6.is_none() {
                    let mut ipv6 = [0u8; 16];
                    ipv6.copy_from_slice(&buf[pos..pos + 16]);
                    result.ipv6 = Some(ipv6);
                }
            }
            _ => {}
        }

        pos += rdlength as usize;
    }

    result
}

/// Resolve a hostname to an IP address
///
/// Returns the first IPv4 address found, or None if resolution fails.
///
/// # Arguments
/// * `hostname` - The hostname to resolve
/// * `dns_server` - Optional DNS server IP (uses 8.8.8.8 if None)
///
/// # Returns
/// IPv4 address as (a, b, c, d) tuple, or None if resolution fails.
pub fn resolve(hostname: &str, dns_server: Option<(u8, u8, u8, u8)>) -> Option<(u8, u8, u8, u8)> {
    // Check if it's already an IP address
    if let Some(ip) = parse_ip(hostname) {
        return Some(ip);
    }

    let server = dns_server.unwrap_or(DEFAULT_DNS_SERVER);

    // Create UDP socket
    let sock = socket(af::INET, sock::DGRAM, ipproto::UDP);
    if sock < 0 {
        return None;
    }

    // Build query
    let mut query = [0u8; 512];
    let query_len = build_query(hostname, DNS_TYPE_A, &mut query);

    // Send query
    let dest = sockaddr_in_octets(DNS_PORT, server.0, server.1, server.2, server.3);
    let sent = sendto(sock, &query[..query_len], 0, &dest, SOCKADDR_IN_SIZE);
    if sent < 0 {
        close(sock);
        return None;
    }

    // Receive response
    let mut response = [0u8; 512];
    let mut src_addr = SockAddrIn::default();
    let mut src_len = SOCKADDR_IN_SIZE;

    let received = recvfrom(
        sock,
        &mut response,
        0,
        Some(&mut src_addr),
        Some(&mut src_len),
    );
    close(sock);

    if received < 0 {
        return None;
    }

    // Parse response
    let result = parse_response(&response, received as usize);
    result.ipv4
}

/// Resolve hostname with full results (IPv4 and IPv6)
///
/// # Arguments
/// * `hostname` - The hostname to resolve
/// * `dns_server` - Optional DNS server IP
///
/// # Returns
/// ResolvedAddr containing both IPv4 and IPv6 addresses if found.
pub fn resolve_full(hostname: &str, dns_server: Option<(u8, u8, u8, u8)>) -> ResolvedAddr {
    let mut result = ResolvedAddr::default();

    // Check if it's already an IP address
    if let Some(ip) = parse_ip(hostname) {
        result.ipv4 = Some(ip);
        return result;
    }

    let server = dns_server.unwrap_or(DEFAULT_DNS_SERVER);

    // Create UDP socket
    let sock = socket(af::INET, sock::DGRAM, ipproto::UDP);
    if sock < 0 {
        return result;
    }

    // Query for A record (IPv4)
    let mut query = [0u8; 512];
    let query_len = build_query(hostname, DNS_TYPE_A, &mut query);
    let dest = sockaddr_in_octets(DNS_PORT, server.0, server.1, server.2, server.3);

    if sendto(sock, &query[..query_len], 0, &dest, SOCKADDR_IN_SIZE) >= 0 {
        let mut response = [0u8; 512];
        let mut src_addr = SockAddrIn::default();
        let mut src_len = SOCKADDR_IN_SIZE;

        let received = recvfrom(
            sock,
            &mut response,
            0,
            Some(&mut src_addr),
            Some(&mut src_len),
        );
        if received > 0 {
            let res = parse_response(&response, received as usize);
            result.ipv4 = res.ipv4;
        }
    }

    // Query for AAAA record (IPv6)
    let query_len = build_query(hostname, DNS_TYPE_AAAA, &mut query);

    if sendto(sock, &query[..query_len], 0, &dest, SOCKADDR_IN_SIZE) >= 0 {
        let mut response = [0u8; 512];
        let mut src_addr = SockAddrIn::default();
        let mut src_len = SOCKADDR_IN_SIZE;

        let received = recvfrom(
            sock,
            &mut response,
            0,
            Some(&mut src_addr),
            Some(&mut src_len),
        );
        if received > 0 {
            let res = parse_response(&response, received as usize);
            result.ipv6 = res.ipv6;
        }
    }

    close(sock);
    result
}

/// Parse an IP address from string (e.g., "192.168.1.1")
fn parse_ip(s: &str) -> Option<(u8, u8, u8, u8)> {
    let mut octets = [0u8; 4];
    let mut octet_idx = 0;
    let mut current: u16 = 0;
    let mut has_digit = false;

    for c in s.bytes() {
        if c == b'.' {
            if !has_digit || octet_idx >= 3 || current > 255 {
                return None;
            }
            octets[octet_idx] = current as u8;
            octet_idx += 1;
            current = 0;
            has_digit = false;
        } else if c >= b'0' && c <= b'9' {
            current = current * 10 + (c - b'0') as u16;
            has_digit = true;
            if current > 255 {
                return None;
            }
        } else {
            return None;
        }
    }

    if !has_digit || octet_idx != 3 || current > 255 {
        return None;
    }
    octets[octet_idx] = current as u8;

    Some((octets[0], octets[1], octets[2], octets[3]))
}

/// POSIX-like gethostbyname interface
///
/// Resolves hostname and returns the first IPv4 address found.
/// This is a simplified version that doesn't return the full hostent structure.
///
/// # Arguments
/// * `name` - Hostname to resolve
///
/// # Returns
/// IPv4 address as u32 in network byte order, or 0 on failure.
pub fn gethostbyname(name: &str) -> u32 {
    match resolve(name, None) {
        Some((a, b, c, d)) => u32::from_be_bytes([a, b, c, d]),
        None => 0,
    }
}

/// Get hostname for a given IP address (reverse DNS)
///
/// Note: Not yet implemented, returns None.
pub fn gethostbyaddr(_addr: u32) -> Option<&'static str> {
    // Reverse DNS lookup would require PTR queries
    // Not implemented yet
    None
}
