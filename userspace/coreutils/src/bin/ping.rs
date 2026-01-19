//! ping - Send ICMP ECHO_REQUEST to network hosts
//!
//! Uses raw sockets to send ICMP echo requests and receive replies.

#![no_std]
#![no_main]

use efflux_libc::{println, print, eprintln, putchar};
use efflux_libc::socket::{
    socket, af, sock, ipproto, sockaddr_in_octets, connect, send, recv,
    htons, SOCKADDR_IN_SIZE,
};
use efflux_libc::time::sleep;
use efflux_libc::close;

const ICMP_ECHO_REQUEST: u8 = 8;
const ICMP_ECHO_REPLY: u8 = 0;

/// Calculate ICMP checksum
fn checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0;

    while i + 1 < data.len() {
        sum += u16::from_be_bytes([data[i], data[i + 1]]) as u32;
        i += 2;
    }

    if i < data.len() {
        sum += (data[i] as u32) << 8;
    }

    while sum >> 16 != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }

    !sum as u16
}

/// Parse IP address from string
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

fn print_ip(ip: (u8, u8, u8, u8)) {
    efflux_libc::print_u64(ip.0 as u64);
    putchar(b'.');
    efflux_libc::print_u64(ip.1 as u64);
    putchar(b'.');
    efflux_libc::print_u64(ip.2 as u64);
    putchar(b'.');
    efflux_libc::print_u64(ip.3 as u64);
}

#[unsafe(no_mangle)]
fn main() -> i32 {
    // Hardcoded target for now
    let target = "10.0.2.2";
    let count = 4;

    let ip = match parse_ip(target) {
        Some(ip) => ip,
        None => {
            eprintln("ping: hostname resolution not implemented, use IP address");
            return 1;
        }
    };

    // Create raw socket for ICMP
    let sock = socket(af::INET, sock::RAW, ipproto::ICMP);
    if sock < 0 {
        print("ping: failed to create socket (need root?): ");
        efflux_libc::print_i64(sock as i64);
        println("");
        return 1;
    }

    print("PING ");
    print_ip(ip);
    println(" 56(84) bytes of data.");

    let addr = sockaddr_in_octets(0, ip.0, ip.1, ip.2, ip.3);

    let ret = connect(sock, &addr, SOCKADDR_IN_SIZE);
    if ret < 0 {
        print("ping: connect failed: ");
        efflux_libc::print_i64(ret as i64);
        println("");
        close(sock);
        return 1;
    }

    let mut sent = 0;
    let mut received = 0;
    let pid: u16 = 1234;

    for seq in 1..=count {
        // Build ICMP echo request
        let mut packet = [0u8; 64];
        packet[0] = ICMP_ECHO_REQUEST;
        packet[1] = 0;  // code
        packet[2] = 0;  // checksum placeholder
        packet[3] = 0;
        packet[4..6].copy_from_slice(&htons(pid).to_be_bytes());
        packet[6..8].copy_from_slice(&htons(seq as u16).to_be_bytes());

        // Calculate and set checksum
        let cksum = checksum(&packet[..64]);
        packet[2..4].copy_from_slice(&cksum.to_be_bytes());

        // Send packet
        let n = send(sock, &packet[..64], 0);
        if n < 0 {
            print("ping: send failed: ");
            efflux_libc::print_i64(n as i64);
            println("");
            break;
        }
        sent += 1;

        // Receive reply
        let mut reply = [0u8; 128];
        let n = recv(sock, &mut reply, 0);

        if n > 0 {
            received += 1;
            let icmp_offset = 20;  // IP header is 20 bytes
            if (n as usize) > icmp_offset {
                let icmp_type = reply[icmp_offset];
                if icmp_type == ICMP_ECHO_REPLY {
                    print("64 bytes from ");
                    print_ip(ip);
                    print(": icmp_seq=");
                    efflux_libc::print_u64(seq as u64);
                    println(" time=<1ms");
                }
            }
        }

        // Sleep 1 second between pings
        if seq < count {
            sleep(1);
        }
    }

    close(sock);

    println("");
    print("--- ");
    print_ip(ip);
    println(" ping statistics ---");

    efflux_libc::print_u64(sent as u64);
    print(" packets transmitted, ");
    efflux_libc::print_u64(received as u64);
    print(" received, ");

    let loss = if sent > 0 {
        ((sent - received) * 100) / sent
    } else {
        0
    };
    efflux_libc::print_u64(loss as u64);
    println("% packet loss");

    if received == 0 { 1 } else { 0 }
}
