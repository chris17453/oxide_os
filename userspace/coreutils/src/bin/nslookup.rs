//! nslookup - Query DNS servers
//!
//! Simple DNS lookup utility.

#![no_std]
#![no_main]

use libc::{printlns, prints, eprintlns, putchar};
use libc::socket::{
    socket, af, sock, ipproto, sockaddr_in_octets, sendto, recvfrom,
    htons, SOCKADDR_IN_SIZE, SockAddrIn,
};
use libc::close;

const DNS_TYPE_A: u16 = 1;
const DNS_CLASS_IN: u16 = 1;
const DNS_RD: u16 = 0x0100;

/// Build DNS query packet
fn build_query(hostname: &str, buf: &mut [u8]) -> usize {
    let mut pos = 0;

    // Transaction ID
    buf[pos] = 0x12;
    buf[pos + 1] = 0x34;
    pos += 2;

    // Flags: standard query with recursion desired
    buf[pos] = (DNS_RD >> 8) as u8;
    buf[pos + 1] = (DNS_RD & 0xFF) as u8;
    pos += 2;

    // QDCOUNT = 1
    buf[pos] = 0;
    buf[pos + 1] = 1;
    pos += 2;

    // ANCOUNT, NSCOUNT, ARCOUNT = 0
    for _ in 0..6 {
        buf[pos] = 0;
        pos += 1;
    }

    // QNAME
    for label in hostname.split('.') {
        let label_bytes = label.as_bytes();
        buf[pos] = label_bytes.len() as u8;
        pos += 1;
        buf[pos..pos + label_bytes.len()].copy_from_slice(label_bytes);
        pos += label_bytes.len();
    }
    buf[pos] = 0;
    pos += 1;

    // QTYPE = A
    buf[pos] = 0;
    buf[pos + 1] = DNS_TYPE_A as u8;
    pos += 2;

    // QCLASS = IN
    buf[pos] = 0;
    buf[pos + 1] = DNS_CLASS_IN as u8;
    pos += 2;

    pos
}

/// Parse DNS response
fn parse_response(buf: &[u8], len: usize) -> Option<(u8, u8, u8, u8)> {
    if len < 12 {
        return None;
    }

    let ancount = ((buf[6] as u16) << 8) | (buf[7] as u16);
    if ancount == 0 {
        return None;
    }

    // Skip header
    let mut pos = 12;

    // Skip question section
    while pos < len && buf[pos] != 0 {
        if buf[pos] & 0xC0 == 0xC0 {
            pos += 2;
            break;
        }
        pos += buf[pos] as usize + 1;
    }
    if pos < len && buf[pos] == 0 {
        pos += 1;
    }
    pos += 4;  // QTYPE + QCLASS

    // Parse first answer
    if pos + 12 > len {
        return None;
    }

    // Skip NAME
    if buf[pos] & 0xC0 == 0xC0 {
        pos += 2;
    } else {
        while pos < len && buf[pos] != 0 {
            pos += buf[pos] as usize + 1;
        }
        pos += 1;
    }

    if pos + 10 > len {
        return None;
    }

    let rtype = ((buf[pos] as u16) << 8) | (buf[pos + 1] as u16);
    pos += 2;
    pos += 2;  // RCLASS
    pos += 4;  // TTL

    let rdlength = ((buf[pos] as u16) << 8) | (buf[pos + 1] as u16);
    pos += 2;

    if rtype == DNS_TYPE_A && rdlength == 4 && pos + 4 <= len {
        return Some((buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]));
    }

    None
}

fn print_ip(ip: (u8, u8, u8, u8)) {
    libc::print_u64(ip.0 as u64);
    putchar(b'.');
    libc::print_u64(ip.1 as u64);
    putchar(b'.');
    libc::print_u64(ip.2 as u64);
    putchar(b'.');
    libc::print_u64(ip.3 as u64);
}

#[unsafe(no_mangle)]
fn main() -> i32 {
    // Hardcoded hostname and DNS server for now
    let hostname = "example.com";
    let dns_server = (8, 8, 8, 8);

    prints("Server:  ");
    print_ip(dns_server);
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
    let query_len = build_query(hostname, &mut query);

    // Send query
    let dest = sockaddr_in_octets(53, dns_server.0, dns_server.1, dns_server.2, dns_server.3);
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

    printlns("Non-authoritative answer:");
    prints("Name: ");
    printlns(hostname);

    match parse_response(&response, received as usize) {
        Some(ip) => {
            prints("Address: ");
            print_ip(ip);
            printlns("");
        }
        None => {
            printlns("No A record found");
        }
    }

    0
}
