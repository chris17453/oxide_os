//! nc (netcat) - Arbitrary TCP and UDP connections and listens
//!
//! A simple implementation of netcat for OXIDE.

#![no_std]
#![no_main]

use libc::{printlns, prints, eprintlns, putchar, getchar};
use libc::socket::{
    socket, bind, listen, accept, connect, recv, shutdown,
    af, sock, ipproto, shut, sockaddr_in_octets, SOCKADDR_IN_SIZE,
    SockAddrIn, ntohs,
};
use libc::close;

/// Parse IP address
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
    libc::print_u64(ip.0 as u64);
    putchar(b'.');
    libc::print_u64(ip.1 as u64);
    putchar(b'.');
    libc::print_u64(ip.2 as u64);
    putchar(b'.');
    libc::print_u64(ip.3 as u64);
}

fn show_help() {
    printlns("Usage: nc [OPTIONS] [hostname] port");
    printlns("       nc -l [OPTIONS] port");
    printlns("");
    printlns("Arbitrary TCP/UDP connections and listens.");
    printlns("");
    printlns("Options:");
    printlns("  -l          Listen mode, for inbound connections");
    printlns("  -u          Use UDP instead of TCP");
    printlns("  -v          Verbose output");
    printlns("  -z          Zero-I/O mode (scan)");
    printlns("  -h          Show this help");
}

/// Transfer data between stdin/stdout and socket
fn transfer_data(sock_fd: i32) {
    let mut buf = [0u8; 4096];

    // Receive data from socket and print
    let n = recv(sock_fd, &mut buf, 0);
    if n > 0 {
        for i in 0..n as usize {
            putchar(buf[i]);
        }
    }
}

#[unsafe(no_mangle)]
fn main() -> i32 {
    // Hardcoded demo: connect to 10.0.2.2:80
    let listen_mode = false;
    let use_udp = false;
    let verbose = true;
    let zero_io = true;

    let ip = (10, 0, 2, 2);
    let port: u16 = 80;

    // Create socket
    let sock_type = if use_udp { sock::DGRAM } else { sock::STREAM };
    let protocol = if use_udp { ipproto::UDP } else { ipproto::TCP };

    let sock = socket(af::INET, sock_type, protocol);
    if sock < 0 {
        prints("nc: failed to create socket: ");
        libc::print_i64(sock as i64);
        printlns("");
        return 1;
    }

    if listen_mode {
        // Server mode
        let bind_addr = sockaddr_in_octets(port, 0, 0, 0, 0);

        let ret = bind(sock, &bind_addr, SOCKADDR_IN_SIZE);
        if ret < 0 {
            prints("nc: bind failed: ");
            libc::print_i64(ret as i64);
            printlns("");
            close(sock);
            return 1;
        }

        if verbose {
            prints("Listening on 0.0.0.0:");
            libc::print_u64(port as u64);
            printlns("");
        }

        if !use_udp {
            let ret = listen(sock, 1);
            if ret < 0 {
                prints("nc: listen failed: ");
                libc::print_i64(ret as i64);
                printlns("");
                close(sock);
                return 1;
            }

            let mut client_addr = SockAddrIn::default();
            let mut addr_len = SOCKADDR_IN_SIZE;

            let client = accept(sock, Some(&mut client_addr), Some(&mut addr_len));
            if client < 0 {
                prints("nc: accept failed: ");
                libc::print_i64(client as i64);
                printlns("");
                close(sock);
                return 1;
            }

            if verbose {
                printlns("Connection received");
            }

            if !zero_io {
                transfer_data(client);
            }

            close(client);
        }
    } else {
        // Client mode
        if verbose {
            prints("Connecting to ");
            print_ip(ip);
            prints(":");
            libc::print_u64(port as u64);
            printlns("...");
        }

        let dest = sockaddr_in_octets(port, ip.0, ip.1, ip.2, ip.3);

        let ret = connect(sock, &dest, SOCKADDR_IN_SIZE);
        if ret < 0 {
            prints("nc: connect failed: ");
            libc::print_i64(ret as i64);
            printlns("");
            close(sock);
            return 1;
        }

        if verbose {
            printlns("Connected!");
        }

        if zero_io {
            prints("Connection to ");
            print_ip(ip);
            prints(":");
            libc::print_u64(port as u64);
            printlns(" succeeded");
        } else {
            transfer_data(sock);
        }
    }

    shutdown(sock, shut::RDWR);
    close(sock);

    0
}
