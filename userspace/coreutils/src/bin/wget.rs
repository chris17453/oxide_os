//! wget - Download files from the web
//!
//! Full-featured implementation with:
//! - Command-line URL parsing
//! - Output to file (-O filename)
//! - Auto-filename from URL (default behavior)
//! - Quiet mode (-q)
//! - Verbose mode (-v)
//! - HTTP/1.1 GET requests
//! - HTTP header parsing
//! - Progress indication
//! - IPv4 address parsing (DNS not yet supported)

#![no_std]
#![no_main]

use libc::*;
use libc::socket::{
    tcp_socket, connect, send, recv, shutdown, sockaddr_in_octets,
    shut, SOCKADDR_IN_SIZE,
};

const MAX_URL: usize = 256;
const MAX_FILENAME: usize = 128;

struct WgetConfig {
    quiet: bool,
    verbose: bool,
    output_file: Option<[u8; MAX_FILENAME]>,
}

impl WgetConfig {
    fn new() -> Self {
        WgetConfig {
            quiet: false,
            verbose: false,
            output_file: None,
        }
    }
}

fn cstr_to_str(ptr: *const u8) -> &'static str {
    if ptr.is_null() {
        return "";
    }
    let mut len = 0;
    unsafe {
        while *ptr.add(len) != 0 {
            len += 1;
        }
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr, len))
    }
}

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

/// Extract filename from URL path
fn extract_filename(path: &str) -> &str {
    // Find last / and take everything after it
    if let Some(idx) = path.rfind('/') {
        let filename = &path[idx + 1..];
        if !filename.is_empty() {
            return filename;
        }
    }
    "index.html"
}

/// Parse port number from string
fn parse_port(s: &str) -> Option<u16> {
    let mut port: u32 = 0;
    for c in s.bytes() {
        if c < b'0' || c > b'9' {
            return None;
        }
        port = port * 10 + (c - b'0') as u32;
        if port > 65535 {
            return None;
        }
    }
    if port == 0 || port > 65535 {
        None
    } else {
        Some(port as u16)
    }
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
    print_u64(ip.0 as u64);
    putchar(b'.');
    print_u64(ip.1 as u64);
    putchar(b'.');
    print_u64(ip.2 as u64);
    putchar(b'.');
    print_u64(ip.3 as u64);
}

fn show_help() {
    eprintlns("Usage: wget [OPTIONS] URL");
    eprintlns("");
    eprintlns("Download files from the web.");
    eprintlns("");
    eprintlns("Options:");
    eprintlns("  -O FILE     Save to FILE (default: extract from URL)");
    eprintlns("  -q          Quiet mode");
    eprintlns("  -v          Verbose mode");
    eprintlns("  -h          Show this help");
    eprintlns("");
    eprintlns("Note: DNS not yet supported, use IP addresses in URL");
}

/// Download from URL
fn do_wget(config: &WgetConfig, url: &str) -> i32 {
    // Parse URL
    let (host, port, path) = match parse_url(url) {
        Some(parsed) => parsed,
        None => {
            eprintlns("wget: invalid URL format (use http://host/path)");
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

    // Determine output filename
    let output_filename = if let Some(ref name_buf) = config.output_file {
        let len = name_buf.iter().position(|&b| b == 0).unwrap_or(MAX_FILENAME);
        core::str::from_utf8(&name_buf[..len]).unwrap_or("download.html")
    } else {
        extract_filename(path)
    };

    if !config.quiet {
        prints("Connecting to ");
        print_ip(ip);
        prints(":");
        print_u64(port as u64);
        printlns("...");
    }

    // Create TCP socket
    let sock = tcp_socket();
    if sock < 0 {
        eprints("wget: failed to create socket: ");
        print_i64(sock as i64);
        eprintlns("");
        return 1;
    }

    // Connect to server
    let addr = sockaddr_in_octets(port, ip.0, ip.1, ip.2, ip.3);
    let ret = connect(sock, &addr, SOCKADDR_IN_SIZE);
    if ret < 0 {
        eprints("wget: failed to connect: ");
        print_i64(ret as i64);
        eprintlns("");
        close(sock);
        return 1;
    }

    if config.verbose {
        printlns("Connected.");
    }

    // Build HTTP request
    let mut request = [0u8; 1024];
    let req_len = build_request(path, host, &mut request);

    if config.verbose {
        printlns("Sending HTTP request...");
    }

    // Send request
    let sent = send(sock, &request[..req_len], 0);
    if sent < 0 {
        eprints("wget: failed to send request: ");
        print_i64(sent as i64);
        eprintlns("");
        close(sock);
        return 1;
    }

    if !config.quiet {
        printlns("HTTP request sent, awaiting response...");
    }

    // Open output file
    let out_fd = open(output_filename, O_WRONLY | O_CREAT | O_TRUNC, 0o644);
    if out_fd < 0 {
        eprints("wget: cannot create file: ");
        prints(output_filename);
        eprintlns("");
        close(sock);
        return 1;
    }

    if !config.quiet {
        prints("Saving to: ");
        printlns(output_filename);
    }

    // Receive response
    let mut buffer = [0u8; 4096];
    let mut total_received = 0;
    let mut header_end = None;
    let mut body_written: usize = 0;

    loop {
        let received = recv(sock, &mut buffer, 0);
        if received < 0 {
            eprints("wget: receive error: ");
            print_i64(received as i64);
            eprintlns("");
            break;
        }
        if received == 0 {
            // Connection closed
            break;
        }

        total_received += received as usize;

        // Look for end of headers if not found yet
        if header_end.is_none() {
            if let Some(pos) = find_pattern(&buffer[..received as usize], b"\r\n\r\n") {
                header_end = Some(pos + 4);

                // Parse status line (first line)
                let headers = &buffer[..pos];
                if let Some(first_line_end) = find_pattern(headers, b"\r\n") {
                    if let Ok(status_line) = core::str::from_utf8(&headers[..first_line_end]) {
                        if !config.quiet {
                            printlns(status_line);
                        }
                    }
                }

                // Write body (everything after headers)
                let body = &buffer[pos + 4..received as usize];
                if !body.is_empty() {
                    let written = write(out_fd, body);
                    if written > 0 {
                        body_written += written as usize;
                    }
                }
            } else {
                // Still in headers, don't write anything yet
            }
        } else {
            // Already past headers, write body
            let written = write(out_fd, &buffer[..received as usize]);
            if written > 0 {
                body_written += written as usize;
            }
        }

        // Progress indication
        if !config.quiet && body_written > 0 {
            prints("\rDownloaded: ");
            print_u64(body_written as u64);
            prints(" bytes");
        }
    }

    if !config.quiet {
        printlns("");
    }

    // Close file
    close(out_fd);

    // Shutdown and close socket
    shutdown(sock, shut::RDWR);
    close(sock);

    if !config.quiet {
        prints("Download complete: ");
        print_u64(body_written as u64);
        prints(" bytes saved to ");
        printlns(output_filename);
    }

    0
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        show_help();
        return 1;
    }

    let mut config = WgetConfig::new();
    let mut arg_idx = 1;
    let mut url: Option<&str> = None;

    // Parse options
    while arg_idx < argc {
        let arg_ptr = unsafe { *argv.add(arg_idx as usize) };
        let arg = cstr_to_str(arg_ptr);

        if arg.starts_with('-') && arg.len() > 1 && arg != "--" {
            if arg == "-O" {
                // Output filename option
                arg_idx += 1;
                if arg_idx >= argc {
                    eprintlns("wget: option -O requires an argument");
                    return 1;
                }
                let filename = cstr_to_str(unsafe { *argv.add(arg_idx as usize) });
                let mut buf = [0u8; MAX_FILENAME];
                let copy_len = if filename.len() > MAX_FILENAME - 1 {
                    MAX_FILENAME - 1
                } else {
                    filename.len()
                };
                buf[..copy_len].copy_from_slice(&filename.as_bytes()[..copy_len]);
                config.output_file = Some(buf);
                arg_idx += 1;
            } else {
                // Parse character flags
                for c in arg[1..].bytes() {
                    match c {
                        b'q' => config.quiet = true,
                        b'v' => config.verbose = true,
                        b'h' => {
                            show_help();
                            return 0;
                        }
                        _ => {
                            eprints("wget: unknown option: -");
                            putchar(c);
                            eprintlns("");
                            return 1;
                        }
                    }
                }
                arg_idx += 1;
            }
        } else {
            // Positional argument - URL
            url = Some(arg);
            arg_idx += 1;
            break;
        }
    }

    // Check that we have a URL
    let url = match url {
        Some(u) => u,
        None => {
            eprintlns("wget: missing URL");
            show_help();
            return 1;
        }
    };

    do_wget(&config, url)
}
