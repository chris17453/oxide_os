//! DNS resolution support
//!
//! Provides hostname resolution using DNS queries.
//!
//! Resolution order:
//! 1. Check if input is already an IP address
//! 2. Check /etc/hosts for static mappings
//! 3. Use DNS servers from /etc/resolv.conf (or fallback to default)

use crate::{O_RDONLY, close, open2, read};
use crate::socket::{
    SOCKADDR_IN_SIZE, SockAddrIn, af, htons, ipproto, recvfrom, sendto, sock, sockaddr_in_octets,
    socket,
};

/// Default DNS server (Google Public DNS)
pub const DEFAULT_DNS_SERVER: (u8, u8, u8, u8) = (8, 8, 8, 8);

/// Path to hosts file
pub const HOSTS_FILE: &str = "/etc/hosts";

/// Path to resolv.conf
pub const RESOLV_CONF: &str = "/etc/resolv.conf";

/// DNS port
pub const DNS_PORT: u16 = 53;

/// Maximum hostname length
pub const MAX_HOSTNAME: usize = 256;

/// DNS record type A (IPv4)
pub const DNS_TYPE_A: u16 = 1;

/// DNS record type PTR (Pointer for reverse DNS)
pub const DNS_TYPE_PTR: u16 = 12;

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

/// Lookup hostname in /etc/hosts
///
/// Returns IPv4 address if found, None otherwise.
pub fn lookup_hosts_file(hostname: &str) -> Option<(u8, u8, u8, u8)> {
    let fd = open2(HOSTS_FILE, O_RDONLY);
    if fd < 0 {
        return None;
    }

    let mut buf = [0u8; 2048];
    let n = read(fd, &mut buf);
    close(fd);

    if n <= 0 {
        return None;
    }

    let content = core::str::from_utf8(&buf[..n as usize]).ok()?;

    // Parse hosts file: each line is "IP hostname [aliases...]"
    for line in content.lines() {
        let line = line.trim();

        // Skip comments and empty lines
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Split by whitespace
        let mut parts = line.split_whitespace();
        let ip_str = parts.next()?;

        // Check if any hostname matches
        for name in parts {
            if name.eq_ignore_ascii_case(hostname) {
                return parse_ip(ip_str);
            }
        }
    }

    None
}

/// Get DNS servers from /etc/resolv.conf
///
/// Returns a list of DNS server IPs (up to 3).
pub fn get_dns_servers() -> [(u8, u8, u8, u8); 3] {
    let mut servers = [DEFAULT_DNS_SERVER; 3];
    let mut count = 0;

    let fd = open2(RESOLV_CONF, O_RDONLY);
    if fd < 0 {
        return servers;
    }

    let mut buf = [0u8; 1024];
    let n = read(fd, &mut buf);
    close(fd);

    if n <= 0 {
        return servers;
    }

    if let Ok(content) = core::str::from_utf8(&buf[..n as usize]) {
        for line in content.lines() {
            let line = line.trim();

            // Skip comments and empty lines
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }

            // Look for "nameserver IP" lines
            if line.starts_with("nameserver") {
                let rest = line["nameserver".len()..].trim();
                if let Some(ip) = parse_ip(rest) {
                    if count < 3 {
                        servers[count] = ip;
                        count += 1;
                    }
                }
            }
        }
    }

    servers
}

/// Get the first configured DNS server
pub fn get_primary_dns_server() -> (u8, u8, u8, u8) {
    get_dns_servers()[0]
}

/// Resolve a hostname to an IP address
///
/// Resolution order:
/// 1. Check if already an IP address
/// 2. Check /etc/hosts
/// 3. Query DNS servers from /etc/resolv.conf
///
/// # Arguments
/// * `hostname` - The hostname to resolve
/// * `dns_server` - Optional DNS server IP (reads from resolv.conf if None)
///
/// # Returns
/// IPv4 address as (a, b, c, d) tuple, or None if resolution fails.
pub fn resolve(hostname: &str, dns_server: Option<(u8, u8, u8, u8)>) -> Option<(u8, u8, u8, u8)> {
    // Check if it's already an IP address
    if let Some(ip) = parse_ip(hostname) {
        return Some(ip);
    }

    // Check /etc/hosts first
    if let Some(ip) = lookup_hosts_file(hostname) {
        return Some(ip);
    }

    // Get DNS server (from parameter, or resolv.conf, or default)
    let server = dns_server.unwrap_or_else(get_primary_dns_server);

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
/// Resolution order:
/// 1. Check if already an IP address
/// 2. Check /etc/hosts
/// 3. Query DNS servers
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

    // Check /etc/hosts first
    if let Some(ip) = lookup_hosts_file(hostname) {
        result.ipv4 = Some(ip);
        return result;
    }

    let server = dns_server.unwrap_or_else(get_primary_dns_server);

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

/// Static buffer for reverse DNS result (thread-unsafe, per POSIX convention)
use core::cell::UnsafeCell;

struct HostnameBuf {
    data: UnsafeCell<[u8; MAX_HOSTNAME]>,
    len: UnsafeCell<usize>,
}

unsafe impl Sync for HostnameBuf {}

static HOSTNAME_BUF: HostnameBuf = HostnameBuf {
    data: UnsafeCell::new([0; MAX_HOSTNAME]),
    len: UnsafeCell::new(0),
};

/// Build PTR (reverse DNS) query packet
///
/// For IPv4 address a.b.c.d, queries for d.c.b.a.in-addr.arpa
fn build_ptr_query(addr: u32, buf: &mut [u8]) -> usize {
    // Extract octets (addr is in network byte order)
    let bytes = addr.to_be_bytes();
    let (a, b, c, d) = (bytes[0], bytes[1], bytes[2], bytes[3]);

    // Build the reverse lookup name: d.c.b.a.in-addr.arpa
    let mut name_buf = [0u8; 64];
    let mut name_pos = 0;

    // Helper to write a number as decimal
    let write_num = |n: u8, buf: &mut [u8], pos: &mut usize| {
        if n >= 100 {
            buf[*pos] = b'0' + n / 100;
            *pos += 1;
            buf[*pos] = b'0' + (n / 10) % 10;
            *pos += 1;
            buf[*pos] = b'0' + n % 10;
            *pos += 1;
        } else if n >= 10 {
            buf[*pos] = b'0' + n / 10;
            *pos += 1;
            buf[*pos] = b'0' + n % 10;
            *pos += 1;
        } else {
            buf[*pos] = b'0' + n;
            *pos += 1;
        }
    };

    // Build: d.c.b.a.in-addr.arpa
    write_num(d, &mut name_buf, &mut name_pos);
    name_buf[name_pos] = b'.';
    name_pos += 1;
    write_num(c, &mut name_buf, &mut name_pos);
    name_buf[name_pos] = b'.';
    name_pos += 1;
    write_num(b, &mut name_buf, &mut name_pos);
    name_buf[name_pos] = b'.';
    name_pos += 1;
    write_num(a, &mut name_buf, &mut name_pos);
    name_buf[name_pos..name_pos + 13].copy_from_slice(b".in-addr.arpa");
    name_pos += 13;

    let hostname = core::str::from_utf8(&name_buf[..name_pos]).unwrap_or("");

    // Build query using the generic function, but with PTR type
    let mut pos = 0;

    // Transaction ID
    let id: u16 = 0x5678;
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

    // QTYPE = PTR
    buf[pos] = (DNS_TYPE_PTR >> 8) as u8;
    buf[pos + 1] = (DNS_TYPE_PTR & 0xFF) as u8;
    pos += 2;

    // QCLASS = IN
    buf[pos] = 0;
    buf[pos + 1] = 1;
    pos += 2;

    pos
}

/// Parse PTR response and extract hostname
///
/// Returns the length of the hostname written to the output buffer, or 0 on failure.
fn parse_ptr_response(response: &[u8], len: usize, out: &mut [u8]) -> usize {
    if len < 12 {
        return 0;
    }

    // Get answer count
    let ancount = ((response[6] as u16) << 8) | (response[7] as u16);
    if ancount == 0 {
        return 0;
    }

    // Skip header (12 bytes)
    let mut pos = 12;

    // Skip question section (QNAME + QTYPE + QCLASS)
    while pos < len && response[pos] != 0 {
        if response[pos] & 0xC0 == 0xC0 {
            pos += 2;
            break;
        }
        pos += response[pos] as usize + 1;
    }
    if pos < len && response[pos] == 0 {
        pos += 1;
    }
    pos += 4; // QTYPE + QCLASS

    // Parse answer records looking for PTR
    for _ in 0..ancount {
        if pos + 12 > len {
            break;
        }

        // Skip NAME (might be compressed)
        if response[pos] & 0xC0 == 0xC0 {
            pos += 2;
        } else {
            while pos < len && response[pos] != 0 {
                pos += response[pos] as usize + 1;
            }
            pos += 1;
        }

        if pos + 10 > len {
            break;
        }

        let rtype = ((response[pos] as u16) << 8) | (response[pos + 1] as u16);
        pos += 2;

        // Skip RCLASS
        pos += 2;

        // Skip TTL
        pos += 4;

        let rdlength = ((response[pos] as u16) << 8) | (response[pos + 1] as u16);
        pos += 2;

        if pos + rdlength as usize > len {
            break;
        }

        if rtype == DNS_TYPE_PTR {
            // RDATA contains a domain name - decode it
            let name_len = decode_dns_name(response, len, pos, out);
            if name_len > 0 {
                return name_len;
            }
        }

        pos += rdlength as usize;
    }

    0
}

/// Decode a DNS name from a response packet
///
/// Handles compression pointers and label format.
/// Returns the number of bytes written to `out`.
fn decode_dns_name(packet: &[u8], packet_len: usize, start: usize, out: &mut [u8]) -> usize {
    let mut pos = start;
    let mut out_pos = 0;
    let mut followed_pointer = false;
    let max_jumps = 10; // Prevent infinite loops
    let mut jumps = 0;

    while pos < packet_len && jumps < max_jumps {
        let len_byte = packet[pos];

        if len_byte == 0 {
            // End of name
            break;
        }

        if len_byte & 0xC0 == 0xC0 {
            // Compression pointer
            if pos + 1 >= packet_len {
                break;
            }
            let offset = (((len_byte & 0x3F) as usize) << 8) | (packet[pos + 1] as usize);
            if offset >= packet_len {
                break;
            }
            pos = offset;
            followed_pointer = true;
            jumps += 1;
            continue;
        }

        // Regular label
        let label_len = len_byte as usize;
        if pos + 1 + label_len > packet_len {
            break;
        }

        // Add dot before label (except for first)
        if out_pos > 0 {
            if out_pos >= out.len() {
                break;
            }
            out[out_pos] = b'.';
            out_pos += 1;
        }

        // Copy label
        if out_pos + label_len > out.len() {
            break;
        }
        out[out_pos..out_pos + label_len].copy_from_slice(&packet[pos + 1..pos + 1 + label_len]);
        out_pos += label_len;

        pos += 1 + label_len;
    }

    // Null terminate
    if out_pos < out.len() {
        out[out_pos] = 0;
    }

    out_pos
}

/// Get hostname for a given IP address (reverse DNS)
///
/// Performs a PTR record lookup to find the hostname associated with an IP address.
///
/// # Arguments
/// * `addr` - IPv4 address in network byte order (e.g., from SockAddrIn.sin_addr)
///
/// # Returns
/// Hostname string, or None if resolution fails.
///
/// # Note
/// This function uses a static buffer and is not thread-safe.
pub fn gethostbyaddr(addr: u32) -> Option<&'static str> {
    gethostbyaddr_with_server(addr, None)
}

/// Get hostname for a given IP address with custom DNS server
///
/// # Arguments
/// * `addr` - IPv4 address in network byte order
/// * `dns_server` - Optional DNS server IP (reads from resolv.conf if None)
///
/// # Returns
/// Hostname string, or None if resolution fails.
pub fn gethostbyaddr_with_server(
    addr: u32,
    dns_server: Option<(u8, u8, u8, u8)>,
) -> Option<&'static str> {
    let server = dns_server.unwrap_or_else(get_primary_dns_server);

    // Create UDP socket
    let sock = socket(af::INET, sock::DGRAM, ipproto::UDP);
    if sock < 0 {
        return None;
    }

    // Build PTR query
    let mut query = [0u8; 512];
    let query_len = build_ptr_query(addr, &mut query);

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

    if received <= 0 {
        return None;
    }

    // Parse response
    unsafe {
        let buf = &mut *HOSTNAME_BUF.data.get();
        let name_len = parse_ptr_response(&response, received as usize, buf);

        if name_len == 0 {
            return None;
        }

        *HOSTNAME_BUF.len.get() = name_len;

        // Return as static str
        core::str::from_utf8(&buf[..name_len]).ok()
    }
}

/// HostEntry structure for POSIX-like gethostbyaddr_r
#[derive(Debug)]
pub struct HostEntry {
    /// Official name of the host
    pub h_name: [u8; MAX_HOSTNAME],
    /// Name length (excluding null terminator)
    pub h_name_len: usize,
    /// Address type (AF_INET)
    pub h_addrtype: i32,
    /// Length of address
    pub h_length: i32,
    /// IPv4 address in network byte order
    pub h_addr: u32,
}

impl Default for HostEntry {
    fn default() -> Self {
        HostEntry {
            h_name: [0; MAX_HOSTNAME],
            h_name_len: 0,
            h_addrtype: af::INET as i32,
            h_length: 4,
            h_addr: 0,
        }
    }
}

impl HostEntry {
    /// Get hostname as a str
    pub fn name(&self) -> Option<&str> {
        if self.h_name_len > 0 {
            core::str::from_utf8(&self.h_name[..self.h_name_len]).ok()
        } else {
            None
        }
    }
}

/// Thread-safe reverse DNS lookup
///
/// # Arguments
/// * `addr` - IPv4 address in network byte order
/// * `result` - Buffer to store the HostEntry result
///
/// # Returns
/// 0 on success, negative error code on failure
pub fn gethostbyaddr_r(addr: u32, result: &mut HostEntry) -> i32 {
    gethostbyaddr_r_with_server(addr, result, None)
}

/// Thread-safe reverse DNS lookup with custom DNS server
///
/// # Arguments
/// * `addr` - IPv4 address in network byte order
/// * `result` - Buffer to store the HostEntry result
/// * `dns_server` - Optional DNS server IP (reads from resolv.conf if None)
///
/// # Returns
/// 0 on success, negative error code on failure
pub fn gethostbyaddr_r_with_server(
    addr: u32,
    result: &mut HostEntry,
    dns_server: Option<(u8, u8, u8, u8)>,
) -> i32 {
    let server = dns_server.unwrap_or_else(get_primary_dns_server);

    // Create UDP socket
    let sock = socket(af::INET, sock::DGRAM, ipproto::UDP);
    if sock < 0 {
        return -1;
    }

    // Build PTR query
    let mut query = [0u8; 512];
    let query_len = build_ptr_query(addr, &mut query);

    // Send query
    let dest = sockaddr_in_octets(DNS_PORT, server.0, server.1, server.2, server.3);
    let sent = sendto(sock, &query[..query_len], 0, &dest, SOCKADDR_IN_SIZE);
    if sent < 0 {
        close(sock);
        return -2;
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

    if received <= 0 {
        return -3;
    }

    // Parse response into result buffer
    let name_len = parse_ptr_response(&response, received as usize, &mut result.h_name);

    if name_len == 0 {
        return -4;
    }

    result.h_name_len = name_len;
    result.h_addrtype = af::INET as i32;
    result.h_length = 4;
    result.h_addr = addr;

    0
}
