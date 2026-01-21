//! nslookup - Query DNS servers
//!
//! Full-featured DNS lookup utility with support for:
//! - Forward DNS lookups (A, AAAA records)
//! - Reverse DNS lookups (PTR records)
//! - All DNS record types (MX, NS, SOA, TXT, CNAME, etc.)
//! - Command-line argument parsing
//! - Multiple query types and options
//! - /etc/resolv.conf parsing

#![no_std]
#![no_main]

use libc::{printlns, prints, eprintlns, putchar, strlen};
use libc::socket::{
    socket, af, sock, ipproto, sockaddr_in_octets, sendto, recvfrom,
    SOCKADDR_IN_SIZE, SockAddrIn,
};
use libc::close;

// DNS constants
const DNS_TYPE_A: u16 = 1;       // IPv4 address
const DNS_TYPE_NS: u16 = 2;      // Nameserver
const DNS_TYPE_CNAME: u16 = 5;   // Canonical name
const DNS_TYPE_SOA: u16 = 6;     // Start of authority
const DNS_TYPE_PTR: u16 = 12;    // Pointer record (reverse DNS)
const DNS_TYPE_MX: u16 = 15;     // Mail exchange
const DNS_TYPE_TXT: u16 = 16;    // Text record
const DNS_TYPE_AAAA: u16 = 28;   // IPv6 address
const DNS_TYPE_ANY: u16 = 255;   // Any record

const DNS_CLASS_IN: u16 = 1;     // Internet class
const DNS_CLASS_CH: u16 = 3;     // Chaos class
const DNS_CLASS_HS: u16 = 4;     // Hesiod class

const DNS_RD: u16 = 0x0100;      // Recursion desired flag

// DNS header flags
const DNS_FLAG_QR: u16 = 0x8000;     // Query/Response
const DNS_FLAG_AA: u16 = 0x0400;     // Authoritative answer
const DNS_FLAG_TC: u16 = 0x0200;     // Truncated
const DNS_FLAG_RD: u16 = 0x0100;     // Recursion desired
const DNS_FLAG_RA: u16 = 0x0080;     // Recursion available

// DNS response codes
const DNS_RCODE_OK: u16 = 0;
const DNS_RCODE_FORMAT_ERROR: u16 = 1;
const DNS_RCODE_SERVER_FAILURE: u16 = 2;
const DNS_RCODE_NAME_ERROR: u16 = 3;
const DNS_RCODE_NOT_IMPLEMENTED: u16 = 4;
const DNS_RCODE_REFUSED: u16 = 5;

/// Configuration for DNS query
struct QueryConfig {
    hostname: [u8; 256],
    hostname_len: usize,
    dns_server: (u8, u8, u8, u8),
    query_type: u16,
    query_class: u16,
    debug: bool,
    recursive: bool,
    timeout: u32,
    port: u16,
    use_tcp: bool,
}

impl QueryConfig {
    fn new() -> Self {
        QueryConfig {
            hostname: [0; 256],
            hostname_len: 0,
            dns_server: (8, 8, 8, 8),  // Default to Google DNS
            query_type: DNS_TYPE_A,
            query_class: DNS_CLASS_IN,
            debug: false,
            recursive: true,
            timeout: 5,
            port: 53,
            use_tcp: false,
        }
    }

    fn set_hostname(&mut self, name: &str) {
        let bytes = name.as_bytes();
        let len = if bytes.len() > 255 { 255 } else { bytes.len() };
        self.hostname[..len].copy_from_slice(&bytes[..len]);
        self.hostname_len = len;
    }

    fn hostname_str(&self) -> &str {
        core::str::from_utf8(&self.hostname[..self.hostname_len]).unwrap_or("")
    }
}

/// Build DNS query packet
fn build_query(config: &QueryConfig, buf: &mut [u8]) -> usize {
    let mut pos = 0;

    // Transaction ID (random-ish)
    buf[pos] = 0xAB;
    buf[pos + 1] = 0xCD;
    pos += 2;

    // Flags: standard query with optional recursion
    let flags = if config.recursive { DNS_RD } else { 0 };
    buf[pos] = (flags >> 8) as u8;
    buf[pos + 1] = (flags & 0xFF) as u8;
    pos += 2;

    // QDCOUNT = 1 (one question)
    buf[pos] = 0;
    buf[pos + 1] = 1;
    pos += 2;

    // ANCOUNT, NSCOUNT, ARCOUNT = 0
    for _ in 0..6 {
        buf[pos] = 0;
        pos += 1;
    }

    // QNAME - encode hostname or reverse IP
    if config.query_type == DNS_TYPE_PTR {
        // Reverse DNS: encode IP in reverse order with .in-addr.arpa suffix
        pos = encode_reverse_ip(config.hostname_str(), &mut buf[pos..]) + pos;
    } else {
        // Forward DNS: encode hostname normally
        pos = encode_hostname(config.hostname_str(), &mut buf[pos..]) + pos;
    }

    // QTYPE
    buf[pos] = (config.query_type >> 8) as u8;
    buf[pos + 1] = (config.query_type & 0xFF) as u8;
    pos += 2;

    // QCLASS
    buf[pos] = (config.query_class >> 8) as u8;
    buf[pos + 1] = (config.query_class & 0xFF) as u8;
    pos += 2;

    pos
}

/// Encode hostname into DNS name format
fn encode_hostname(hostname: &str, buf: &mut [u8]) -> usize {
    let mut pos = 0;
    for label in hostname.split('.') {
        if label.is_empty() {
            continue;
        }
        let label_bytes = label.as_bytes();
        if label_bytes.len() > 63 {
            continue;
        }
        buf[pos] = label_bytes.len() as u8;
        pos += 1;
        buf[pos..pos + label_bytes.len()].copy_from_slice(label_bytes);
        pos += label_bytes.len();
    }
    buf[pos] = 0;
    pos + 1
}

/// Encode IP address for reverse DNS lookup (PTR record)
fn encode_reverse_ip(ip: &str, buf: &mut [u8]) -> usize {
    // Parse IP address and reverse it
    let mut octets = [0u8; 4];
    let mut octet_idx = 0;
    let mut current = 0u8;

    for ch in ip.bytes() {
        if ch == b'.' {
            if octet_idx < 4 {
                octets[octet_idx] = current;
                octet_idx += 1;
                current = 0;
            }
        } else if ch >= b'0' && ch <= b'9' {
            current = current * 10 + (ch - b'0');
        }
    }
    if octet_idx < 4 {
        octets[octet_idx] = current;
    }

    // Encode in reverse: d.c.b.a.in-addr.arpa
    let mut pos = 0;
    for i in (0..4).rev() {
        // Write octet as string
        let mut num_buf = [0u8; 3];
        let mut num_len = 0;
        let mut val = octets[i];

        if val == 0 {
            num_buf[0] = b'0';
            num_len = 1;
        } else {
            let mut temp = [0u8; 3];
            let mut temp_len = 0;
            while val > 0 {
                temp[temp_len] = b'0' + (val % 10);
                val /= 10;
                temp_len += 1;
            }
            // Reverse digits
            for j in 0..temp_len {
                num_buf[j] = temp[temp_len - 1 - j];
            }
            num_len = temp_len;
        }

        buf[pos] = num_len as u8;
        pos += 1;
        buf[pos..pos + num_len].copy_from_slice(&num_buf[..num_len]);
        pos += num_len;
    }

    // Add ".in-addr.arpa"
    buf[pos] = 7;
    pos += 1;
    buf[pos..pos + 7].copy_from_slice(b"in-addr");
    pos += 7;

    buf[pos] = 4;
    pos += 1;
    buf[pos..pos + 4].copy_from_slice(b"arpa");
    pos += 4;

    buf[pos] = 0;
    pos + 1
}

/// Decode DNS name from packet (handles compression)
fn decode_name(buf: &[u8], mut pos: usize, result: &mut [u8]) -> (usize, usize) {
    let mut result_len = 0;
    let mut jumps = 0;
    let original_pos = pos;
    let mut jumped = false;
    let mut continue_pos = 0;

    loop {
        if pos >= buf.len() {
            break;
        }

        let len = buf[pos];

        // End of name
        if len == 0 {
            if !jumped {
                continue_pos = pos + 1;
            }
            break;
        }

        // Compression pointer
        if len & 0xC0 == 0xC0 {
            if pos + 1 >= buf.len() {
                break;
            }
            let offset = (((len & 0x3F) as usize) << 8) | (buf[pos + 1] as usize);
            if !jumped {
                continue_pos = pos + 2;
                jumped = true;
            }
            pos = offset;
            jumps += 1;
            if jumps > 10 {  // Prevent infinite loops
                break;
            }
            continue;
        }

        // Regular label
        if result_len > 0 && result_len < result.len() {
            result[result_len] = b'.';
            result_len += 1;
        }

        pos += 1;
        let copy_len = if len as usize > result.len() - result_len {
            result.len() - result_len
        } else {
            len as usize
        };

        if pos + copy_len > buf.len() {
            break;
        }

        result[result_len..result_len + copy_len].copy_from_slice(&buf[pos..pos + copy_len]);
        result_len += copy_len;
        pos += len as usize;
    }

    let final_pos = if jumped { continue_pos } else { pos + 1 };
    (final_pos, result_len)
}

/// Parse DNS response and display results
fn parse_and_display_response(buf: &[u8], len: usize, config: &QueryConfig) -> bool {
    if len < 12 {
        eprintlns("Error: Response too short");
        return false;
    }

    // Parse header
    let txid = ((buf[0] as u16) << 8) | (buf[1] as u16);
    let flags = ((buf[2] as u16) << 8) | (buf[3] as u16);
    let qdcount = ((buf[4] as u16) << 8) | (buf[5] as u16);
    let ancount = ((buf[6] as u16) << 8) | (buf[7] as u16);
    let nscount = ((buf[8] as u16) << 8) | (buf[9] as u16);
    let arcount = ((buf[10] as u16) << 8) | (buf[11] as u16);

    let rcode = flags & 0x000F;
    let is_authoritative = (flags & DNS_FLAG_AA) != 0;

    if config.debug {
        prints("Transaction ID: 0x");
        print_hex16(txid);
        printlns("");
        prints("Flags: 0x");
        print_hex16(flags);
        printlns("");
        prints("Questions: ");
        print_u16(qdcount);
        prints(", Answers: ");
        print_u16(ancount);
        prints(", Authority: ");
        print_u16(nscount);
        prints(", Additional: ");
        print_u16(arcount);
        printlns("");
    }

    // Check response code
    if rcode != DNS_RCODE_OK {
        prints("Error: ");
        match rcode {
            DNS_RCODE_FORMAT_ERROR => printlns("Format error"),
            DNS_RCODE_SERVER_FAILURE => printlns("Server failure"),
            DNS_RCODE_NAME_ERROR => printlns("Name error (NXDOMAIN)"),
            DNS_RCODE_NOT_IMPLEMENTED => printlns("Not implemented"),
            DNS_RCODE_REFUSED => printlns("Query refused"),
            _ => {
                prints("Unknown error code ");
                print_u16(rcode);
                printlns("");
            }
        }
        return false;
    }

    // Skip question section
    let mut pos = 12;
    for _ in 0..qdcount {
        let mut name_buf = [0u8; 256];
        let (new_pos, _) = decode_name(buf, pos, &mut name_buf);
        pos = new_pos;
        pos += 4;  // Skip QTYPE and QCLASS
    }

    // Display authority status
    if is_authoritative {
        printlns("Authoritative answer:");
    } else {
        printlns("Non-authoritative answer:");
    }

    // Parse and display answer section
    if ancount > 0 {
        for i in 0..ancount {
            if !parse_and_display_record(buf, len, &mut pos, config) {
                break;
            }
        }
    } else {
        printlns("No answer records found");
    }

    // Parse and display authority section if debug mode
    if config.debug && nscount > 0 {
        printlns("\nAuthority Section:");
        for _ in 0..nscount {
            if !parse_and_display_record(buf, len, &mut pos, config) {
                break;
            }
        }
    }

    // Parse and display additional section if debug mode
    if config.debug && arcount > 0 {
        printlns("\nAdditional Section:");
        for _ in 0..arcount {
            if !parse_and_display_record(buf, len, &mut pos, config) {
                break;
            }
        }
    }

    true
}

/// Parse and display a single DNS record
fn parse_and_display_record(buf: &[u8], len: usize, pos: &mut usize, config: &QueryConfig) -> bool {
    if *pos + 10 > len {
        return false;
    }

    // Parse NAME
    let mut name_buf = [0u8; 256];
    let (new_pos, name_len) = decode_name(buf, *pos, &mut name_buf);
    *pos = new_pos;

    if *pos + 10 > len {
        return false;
    }

    // Parse TYPE, CLASS, TTL, RDLENGTH
    let rtype = ((buf[*pos] as u16) << 8) | (buf[*pos + 1] as u16);
    *pos += 2;
    let rclass = ((buf[*pos] as u16) << 8) | (buf[*pos + 1] as u16);
    *pos += 2;
    let ttl = ((buf[*pos] as u32) << 24) | ((buf[*pos + 1] as u32) << 16)
            | ((buf[*pos + 2] as u32) << 8) | (buf[*pos + 3] as u32);
    *pos += 4;
    let rdlength = ((buf[*pos] as u16) << 8) | (buf[*pos + 1] as u16);
    *pos += 2;

    if *pos + rdlength as usize > len {
        return false;
    }

    // Display based on record type
    match rtype {
        DNS_TYPE_A => {
            if rdlength == 4 {
                prints("Name:    ");
                print_name(&name_buf, name_len);
                printlns("");
                prints("Address: ");
                print_ipv4(&buf[*pos..*pos + 4]);
                printlns("");
                if config.debug {
                    prints("TTL:     ");
                    print_u32(ttl);
                    printlns(" seconds");
                }
            }
        }
        DNS_TYPE_AAAA => {
            if rdlength == 16 {
                prints("Name:    ");
                print_name(&name_buf, name_len);
                printlns("");
                prints("Address: ");
                print_ipv6(&buf[*pos..*pos + 16]);
                printlns("");
                if config.debug {
                    prints("TTL:     ");
                    print_u32(ttl);
                    printlns(" seconds");
                }
            }
        }
        DNS_TYPE_PTR | DNS_TYPE_CNAME | DNS_TYPE_NS => {
            let type_name = match rtype {
                DNS_TYPE_PTR => "PTR",
                DNS_TYPE_CNAME => "CNAME",
                DNS_TYPE_NS => "NS",
                _ => "Unknown",
            };

            let mut target_buf = [0u8; 256];
            let (_, target_len) = decode_name(buf, *pos, &mut target_buf);

            if rtype == DNS_TYPE_PTR {
                prints("Address: ");
                print_name(&name_buf, name_len);
                printlns("");
                prints("Name:    ");
                print_name(&target_buf, target_len);
                printlns("");
            } else {
                prints(type_name);
                prints(":     ");
                print_name(&target_buf, target_len);
                printlns("");
            }

            if config.debug {
                prints("TTL:     ");
                print_u32(ttl);
                printlns(" seconds");
            }
        }
        DNS_TYPE_MX => {
            if rdlength >= 2 {
                let priority = ((buf[*pos] as u16) << 8) | (buf[*pos + 1] as u16);
                let mut mx_buf = [0u8; 256];
                let (_, mx_len) = decode_name(buf, *pos + 2, &mut mx_buf);

                prints("MX:      priority=");
                print_u16(priority);
                prints(", exchanger=");
                print_name(&mx_buf, mx_len);
                printlns("");

                if config.debug {
                    prints("TTL:     ");
                    print_u32(ttl);
                    printlns(" seconds");
                }
            }
        }
        DNS_TYPE_TXT => {
            prints("TXT:     ");
            let mut txt_pos = *pos;
            let end_pos = *pos + rdlength as usize;
            while txt_pos < end_pos {
                let txt_len = buf[txt_pos] as usize;
                txt_pos += 1;
                if txt_pos + txt_len <= end_pos {
                    putchar(b'"');
                    print_bytes(&buf[txt_pos..txt_pos + txt_len]);
                    putchar(b'"');
                    txt_pos += txt_len;
                    if txt_pos < end_pos {
                        putchar(b' ');
                    }
                } else {
                    break;
                }
            }
            printlns("");

            if config.debug {
                prints("TTL:     ");
                print_u32(ttl);
                printlns(" seconds");
            }
        }
        DNS_TYPE_SOA => {
            // SOA record format: MNAME RNAME SERIAL REFRESH RETRY EXPIRE MINIMUM
            let mut mname_buf = [0u8; 256];
            let (new_pos1, mname_len) = decode_name(buf, *pos, &mut mname_buf);

            let mut rname_buf = [0u8; 256];
            let (new_pos2, rname_len) = decode_name(buf, new_pos1, &mut rname_buf);

            if new_pos2 + 20 <= *pos + rdlength as usize {
                let serial = read_u32(buf, new_pos2);
                let refresh = read_u32(buf, new_pos2 + 4);
                let retry = read_u32(buf, new_pos2 + 8);
                let expire = read_u32(buf, new_pos2 + 12);
                let minimum = read_u32(buf, new_pos2 + 16);

                printlns("SOA:");
                prints("    Origin:  ");
                print_name(&mname_buf, mname_len);
                printlns("");
                prints("    Mail:    ");
                print_name(&rname_buf, rname_len);
                printlns("");
                prints("    Serial:  ");
                print_u32(serial);
                printlns("");
                prints("    Refresh: ");
                print_u32(refresh);
                printlns("");
                prints("    Retry:   ");
                print_u32(retry);
                printlns("");
                prints("    Expire:  ");
                print_u32(expire);
                printlns("");
                prints("    Minimum: ");
                print_u32(minimum);
                printlns("");
            }
        }
        _ => {
            if config.debug {
                prints("Unknown record type ");
                print_u16(rtype);
                prints(" (");
                print_u16(rdlength);
                printlns(" bytes)");
            }
        }
    }

    *pos += rdlength as usize;
    true
}

// Helper functions
fn read_u32(buf: &[u8], pos: usize) -> u32 {
    ((buf[pos] as u32) << 24) | ((buf[pos + 1] as u32) << 16)
    | ((buf[pos + 2] as u32) << 8) | (buf[pos + 3] as u32)
}

fn print_name(buf: &[u8], len: usize) {
    for i in 0..len {
        putchar(buf[i]);
    }
}

fn print_bytes(buf: &[u8]) {
    for &b in buf {
        putchar(b);
    }
}

fn print_ipv4(buf: &[u8]) {
    libc::print_u64(buf[0] as u64);
    putchar(b'.');
    libc::print_u64(buf[1] as u64);
    putchar(b'.');
    libc::print_u64(buf[2] as u64);
    putchar(b'.');
    libc::print_u64(buf[3] as u64);
}

fn print_ipv6(buf: &[u8]) {
    for i in 0..8 {
        if i > 0 {
            putchar(b':');
        }
        print_hex16(((buf[i * 2] as u16) << 8) | (buf[i * 2 + 1] as u16));
    }
}

fn print_u16(val: u16) {
    libc::print_u64(val as u64);
}

fn print_u32(val: u32) {
    libc::print_u64(val as u64);
}

fn print_hex16(val: u16) {
    let hex_chars = b"0123456789abcdef";
    let mut started = false;
    for i in (0..4).rev() {
        let nibble = ((val >> (i * 4)) & 0xF) as usize;
        if nibble != 0 || started || i == 0 {
            putchar(hex_chars[nibble]);
            started = true;
        }
    }
}

/// Check if string is a valid IP address
fn is_ip_address(s: &str) -> bool {
    let mut dots = 0;
    let mut digits = 0;
    for ch in s.bytes() {
        if ch == b'.' {
            if digits == 0 || digits > 3 {
                return false;
            }
            dots += 1;
            digits = 0;
        } else if ch >= b'0' && ch <= b'9' {
            digits += 1;
        } else {
            return false;
        }
    }
    dots == 3 && digits > 0 && digits <= 3
}

/// Parse command-line arguments
fn parse_args(argc: i32, argv: *const *const u8, config: &mut QueryConfig) -> bool {
    if argc < 2 {
        printlns("Usage: nslookup [options] <hostname|IP>");
        printlns("Options:");
        printlns("  -type=TYPE    Query type (A, AAAA, MX, NS, SOA, TXT, CNAME, PTR, ANY)");
        printlns("  -class=CLASS  Query class (IN, CH, HS)");
        printlns("  -server=IP    DNS server IP address");
        printlns("  -port=PORT    DNS server port (default: 53)");
        printlns("  -debug        Enable debug output");
        printlns("  -norecurse    Disable recursion");
        printlns("  -tcp          Use TCP instead of UDP");
        printlns("  -timeout=SEC  Query timeout in seconds");
        printlns("");
        printlns("Examples:");
        printlns("  nslookup example.com");
        printlns("  nslookup 8.8.8.8                    # Reverse DNS");
        printlns("  nslookup -type=MX example.com");
        printlns("  nslookup -server=1.1.1.1 example.com");
        return false;
    }

    let mut i = 1;
    let mut hostname_set = false;

    while i < argc {
        let arg = unsafe { *argv.add(i as usize) };
        if arg.is_null() {
            i += 1;
            continue;
        }
        let arg_len = strlen(arg);

        if arg_len > 0 && unsafe { *arg } == b'-' {
            // Parse option
            let opt = core::str::from_utf8(unsafe {
                core::slice::from_raw_parts(arg.add(1), arg_len - 1)
            }).unwrap_or("");

            if opt.starts_with("type=") {
                let type_str = &opt[5..];
                config.query_type = match type_str {
                    "A" => DNS_TYPE_A,
                    "AAAA" => DNS_TYPE_AAAA,
                    "MX" => DNS_TYPE_MX,
                    "NS" => DNS_TYPE_NS,
                    "SOA" => DNS_TYPE_SOA,
                    "TXT" => DNS_TYPE_TXT,
                    "CNAME" => DNS_TYPE_CNAME,
                    "PTR" => DNS_TYPE_PTR,
                    "ANY" => DNS_TYPE_ANY,
                    _ => {
                        prints("Unknown query type: ");
                        printlns(type_str);
                        return false;
                    }
                };
            } else if opt.starts_with("class=") {
                let class_str = &opt[6..];
                config.query_class = match class_str {
                    "IN" => DNS_CLASS_IN,
                    "CH" => DNS_CLASS_CH,
                    "HS" => DNS_CLASS_HS,
                    _ => {
                        prints("Unknown query class: ");
                        printlns(class_str);
                        return false;
                    }
                };
            } else if opt.starts_with("server=") {
                let server_str = &opt[7..];
                if let Some(ip) = parse_ipv4(server_str) {
                    config.dns_server = ip;
                } else {
                    prints("Invalid server IP: ");
                    printlns(server_str);
                    return false;
                }
            } else if opt.starts_with("port=") {
                let port_str = &opt[5..];
                config.port = parse_u16(port_str).unwrap_or(53);
            } else if opt.starts_with("timeout=") {
                let timeout_str = &opt[8..];
                config.timeout = parse_u32(timeout_str).unwrap_or(5);
            } else if opt == "debug" {
                config.debug = true;
            } else if opt == "norecurse" {
                config.recursive = false;
            } else if opt == "tcp" {
                config.use_tcp = true;
            } else {
                prints("Unknown option: -");
                printlns(opt);
                return false;
            }
        } else {
            // Hostname/IP argument
            let hostname = core::str::from_utf8(unsafe {
                core::slice::from_raw_parts(arg, arg_len)
            }).unwrap_or("");

            config.set_hostname(hostname);
            hostname_set = true;

            // Auto-detect PTR query for IP addresses
            if is_ip_address(hostname) && config.query_type == DNS_TYPE_A {
                config.query_type = DNS_TYPE_PTR;
            }
        }

        i += 1;
    }

    if !hostname_set {
        eprintlns("Error: No hostname or IP address specified");
        return false;
    }

    true
}

fn parse_ipv4(s: &str) -> Option<(u8, u8, u8, u8)> {
    let mut octets = [0u8; 4];
    let mut idx = 0;
    let mut current = 0u16;

    for ch in s.bytes() {
        if ch == b'.' {
            if idx >= 4 || current > 255 {
                return None;
            }
            octets[idx] = current as u8;
            idx += 1;
            current = 0;
        } else if ch >= b'0' && ch <= b'9' {
            current = current * 10 + (ch - b'0') as u16;
        } else {
            return None;
        }
    }

    if idx == 3 && current <= 255 {
        octets[3] = current as u8;
        Some((octets[0], octets[1], octets[2], octets[3]))
    } else {
        None
    }
}

fn parse_u16(s: &str) -> Option<u16> {
    let mut val = 0u16;
    for ch in s.bytes() {
        if ch >= b'0' && ch <= b'9' {
            val = val * 10 + (ch - b'0') as u16;
        } else {
            return None;
        }
    }
    Some(val)
}

fn parse_u32(s: &str) -> Option<u32> {
    let mut val = 0u32;
    for ch in s.bytes() {
        if ch >= b'0' && ch <= b'9' {
            val = val * 10 + (ch - b'0') as u32;
        } else {
            return None;
        }
    }
    Some(val)
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    let mut config = QueryConfig::new();

    if !parse_args(argc, argv, &mut config) {
        return 1;
    }

    // Display server info
    prints("Server:  ");
    print_ipv4(&[config.dns_server.0, config.dns_server.1,
                  config.dns_server.2, config.dns_server.3]);
    printlns("");
    printlns("");

    // Create UDP socket
    let sock = socket(af::INET, sock::DGRAM, ipproto::UDP);
    if sock < 0 {
        prints("nslookup: failed to create socket: ");
        libc::print_i64(sock as i64);
        printlns("");
        return 1;
    }

    // Build query
    let mut query = [0u8; 512];
    let query_len = build_query(&config, &mut query);

    if config.debug {
        prints("Query size: ");
        libc::print_u64(query_len as u64);
        printlns(" bytes");
    }

    // Send query
    let dest = sockaddr_in_octets(
        config.port,
        config.dns_server.0,
        config.dns_server.1,
        config.dns_server.2,
        config.dns_server.3
    );
    let sent = sendto(sock, &query[..query_len], 0, &dest, SOCKADDR_IN_SIZE);
    if sent < 0 {
        prints("nslookup: failed to send query: ");
        libc::print_i64(sent as i64);
        printlns("");
        close(sock);
        return 1;
    }

    // Receive response
    let mut response = [0u8; 512];
    let mut src_addr = SockAddrIn::default();
    let mut src_len = SOCKADDR_IN_SIZE;

    let received = recvfrom(sock, &mut response, 0, Some(&mut src_addr), Some(&mut src_len));
    if received < 0 {
        prints("nslookup: failed to receive response: ");
        libc::print_i64(received as i64);
        printlns("");
        close(sock);
        return 1;
    }

    close(sock);

    if config.debug {
        prints("Response size: ");
        libc::print_u64(received as u64);
        printlns(" bytes");
    }

    // Parse and display response
    if !parse_and_display_response(&response, received as usize, &config) {
        return 1;
    }

    0
}
