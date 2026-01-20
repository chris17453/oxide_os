//! wget - Download files from the web
//!
//! A minimal wget implementation for OXIDE.

#![no_std]
#![no_main]

use libc::{
    printlns, eprintlns, prints, putchar,
    socket::{
        tcp_socket, connect, send, recv, shutdown, sockaddr_in_octets,
        shut, SOCKADDR_IN_SIZE,
    },
    close,
};

/// Parse a simple URL: http://host[:port]/path
/// Returns (host, port, path) or None if invalid
fn parse_url(url: &str) -> Option<(&str, u16, &str)> {
    // Must start with http://
    let url = url.strip_prefix("http://")?;

    // Split host from path
    let (host_port, path) = if let Some(idx) = url.find('/') {
        (&url[..idx], &url[idx..])
    } else {
        (url, "/")
    };

    // Check for port
    let (host, port) = if let Some(idx) = host_port.find(':') {
        let port_str = &host_port[idx + 1..];
        let port = parse_port(port_str)?;
        (&host_port[..idx], port)
    } else {
        (host_port, 80)
    };

    Some((host, port, path))
}

/// Parse port number from string
fn parse_port(s: &str) -> Option<u16> {
    let mut port: u16 = 0;
    for c in s.bytes() {
        if c < b'0' || c > b'9' {
            return None;
        }
        port = port.checked_mul(10)?;
        port = port.checked_add((c - b'0') as u16)?;
    }
    if port == 0 {
        return None;
    }
    Some(port)
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

/// Build an HTTP GET request
fn build_request(path: &str, host: &str, buf: &mut [u8]) -> usize {
    let mut len = 0;

    // GET /path HTTP/1.1\r\n
    len += copy_str(&mut buf[len..], "GET ");
    len += copy_str(&mut buf[len..], path);
    len += copy_str(&mut buf[len..], " HTTP/1.1\r\n");

    // Host: host\r\n
    len += copy_str(&mut buf[len..], "Host: ");
    len += copy_str(&mut buf[len..], host);
    len += copy_str(&mut buf[len..], "\r\n");

    // User-Agent
    len += copy_str(&mut buf[len..], "User-Agent: wget/oxide\r\n");

    // Connection: close
    len += copy_str(&mut buf[len..], "Connection: close\r\n");

    // End of headers
    len += copy_str(&mut buf[len..], "\r\n");

    len
}

/// Copy string to buffer
fn copy_str(buf: &mut [u8], s: &str) -> usize {
    let bytes = s.as_bytes();
    let len = bytes.len().min(buf.len());
    buf[..len].copy_from_slice(&bytes[..len]);
    len
}

/// Find pattern in buffer
fn find_pattern(buf: &[u8], pattern: &[u8]) -> Option<usize> {
    if pattern.len() > buf.len() {
        return None;
    }
    for i in 0..=(buf.len() - pattern.len()) {
        if &buf[i..i + pattern.len()] == pattern {
            return Some(i);
        }
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
    // Hardcoded URL for now - argument parsing would need env support
    // Users would call: wget http://192.168.1.1/path
    let url = "http://10.0.2.2/";

    // Parse URL
    let (host, port, path) = match parse_url(url) {
        Some(parsed) => parsed,
        None => {
            eprintlns("wget: invalid URL format");
            return 1;
        }
    };

    // For now, host must be an IP address (DNS not yet implemented)
    let ip = match parse_ip(host) {
        Some(ip) => ip,
        None => {
            eprintlns("wget: hostname resolution not implemented, use IP address");
            return 1;
        }
    };

    prints("Connecting to ");
    print_ip(ip);
    prints(":");
    libc::print_u64(port as u64);
    printlns("...");

    // Create TCP socket
    let sock = tcp_socket();
    if sock < 0 {
        prints("wget: failed to create socket: ");
        libc::print_i64(sock as i64);
        printlns("");
        return 1;
    }

    // Connect to server
    let addr = sockaddr_in_octets(port, ip.0, ip.1, ip.2, ip.3);
    let ret = connect(sock, &addr, SOCKADDR_IN_SIZE);
    if ret < 0 {
        prints("wget: failed to connect: ");
        libc::print_i64(ret as i64);
        printlns("");
        close(sock);
        return 1;
    }

    printlns("Connected.");

    // Build HTTP request
    let mut request = [0u8; 1024];
    let req_len = build_request(path, host, &mut request);

    // Send request
    let sent = send(sock, &request[..req_len], 0);
    if sent < 0 {
        prints("wget: failed to send request: ");
        libc::print_i64(sent as i64);
        printlns("");
        close(sock);
        return 1;
    }

    printlns("HTTP request sent, awaiting response...");

    // Receive response
    let mut buffer = [0u8; 4096];
    let mut total_received = 0;
    let mut header_end = None;
    let mut body_received: usize = 0;

    loop {
        let received = recv(sock, &mut buffer[total_received..], 0);
        if received < 0 {
            prints("wget: receive error: ");
            libc::print_i64(received as i64);
            printlns("");
            break;
        }
        if received == 0 {
            // Connection closed
            break;
        }

        total_received += received as usize;

        // Look for end of headers if not found yet
        if header_end.is_none() {
            if let Some(pos) = find_pattern(&buffer[..total_received], b"\r\n\r\n") {
                header_end = Some(pos + 4);

                // Parse status line (first line)
                let headers = &buffer[..pos];
                if let Some(first_line_end) = find_pattern(headers, b"\r\n") {
                    if let Ok(status_line) = core::str::from_utf8(&headers[..first_line_end]) {
                        printlns(status_line);
                    }
                }

                // Output body (everything after headers)
                let body = &buffer[pos + 4..total_received];
                body_received = body.len();
                print_bytes(body);
            }
        } else {
            // Already have headers, just output body
            print_bytes(&buffer[total_received - received as usize..total_received]);
            body_received += received as usize;
        }

        // If buffer is full, reset for more data
        if total_received >= buffer.len() {
            total_received = 0;
        }
    }

    // Shutdown and close
    shutdown(sock, shut::RDWR);
    close(sock);

    printlns("");
    prints("Downloaded ");
    libc::print_u64(body_received as u64);
    printlns(" bytes.");

    0
}

/// Print bytes to stdout
fn print_bytes(bytes: &[u8]) {
    for &b in bytes {
        putchar(b);
    }
}
