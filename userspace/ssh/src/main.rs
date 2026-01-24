//! OXIDE SSH Client
//!
//! Usage: ssh [options] [user@]hostname [command]
//!
//! Options:
//!   -p port    Connect to this port (default: 22)
//!   -l user    Login as this user
//!   -v         Verbose mode
//!   -o option  Set option (e.g., StrictHostKeyChecking=no)

#![no_std]
#![no_main]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use libc::dns;
use libc::socket::{InAddr, SOCKADDR_IN_SIZE, SockAddrIn, af, connect, sock, socket};
use libc::*;

mod crypto;
mod kex;
mod session;
mod transport;

use kex::perform_key_exchange;
use session::{SshChannel, authenticate_password, request_userauth_service, run_session};
use transport::{SshTransport, TransportError};

/// SSH client configuration
struct SshConfig {
    /// Remote hostname
    hostname: String,
    /// Remote port
    port: u16,
    /// Username
    username: String,
    /// Verbose mode
    verbose: bool,
    /// Skip host key verification
    skip_host_key_check: bool,
}

impl SshConfig {
    fn new() -> Self {
        SshConfig {
            hostname: String::new(),
            port: 22,
            username: String::new(),
            verbose: false,
            skip_host_key_check: false,
        }
    }
}

/// Convert C string to str
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

/// Main entry point
#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        usage();
        return 1;
    }

    let config = match parse_args(argc, argv) {
        Some(c) => c,
        None => {
            usage();
            return 1;
        }
    };

    if config.hostname.is_empty() {
        eprintlns("ssh: missing hostname");
        return 1;
    }

    // Get username if not specified
    let username = if config.username.is_empty() {
        get_current_username()
    } else {
        config.username.clone()
    };

    if config.verbose {
        prints("ssh: connecting to ");
        prints_str(&config.hostname);
        prints(":");
        print_i64(config.port as i64);
        printlns("");
    }

    // Resolve hostname
    let ip = match resolve_hostname(&config.hostname) {
        Some(ip) => ip,
        None => {
            prints("ssh: could not resolve hostname: ");
            printlns_str(&config.hostname);
            return 1;
        }
    };

    if config.verbose {
        prints("ssh: resolved to ");
        print_i64(((ip >> 24) & 0xff) as i64);
        prints(".");
        print_i64(((ip >> 16) & 0xff) as i64);
        prints(".");
        print_i64(((ip >> 8) & 0xff) as i64);
        prints(".");
        print_i64((ip & 0xff) as i64);
        printlns("");
    }

    // Connect to server
    let fd = match connect_to_server(ip, config.port) {
        Ok(fd) => fd,
        Err(e) => {
            prints("ssh: connection failed: ");
            printlns(e);
            return 1;
        }
    };

    if config.verbose {
        printlns("ssh: connected, starting SSH handshake");
    }

    // Create transport
    let mut transport = match SshTransport::new(fd) {
        Ok(t) => t,
        Err(_) => {
            eprintlns("ssh: failed to create transport");
            close(fd);
            return 1;
        }
    };

    // Version exchange
    if config.verbose {
        printlns("ssh: version exchange");
    }
    if let Err(e) = transport.version_exchange() {
        print_error("version exchange", e);
        close(fd);
        return 1;
    }

    // Key exchange
    if config.verbose {
        printlns("ssh: key exchange");
    }
    if let Err(e) = perform_key_exchange(&mut transport) {
        print_error("key exchange", e);
        close(fd);
        return 1;
    }

    if config.verbose {
        printlns("ssh: key exchange complete, requesting authentication");
    }

    // Request userauth service
    if let Err(e) = request_userauth_service(&mut transport) {
        print_error("service request", e);
        close(fd);
        return 1;
    }

    // Get password
    let password = read_password(&username, &config.hostname);

    // Authenticate
    if config.verbose {
        printlns("ssh: authenticating");
    }
    if let Err(e) = authenticate_password(&mut transport, username.as_bytes(), password.as_bytes())
    {
        match e {
            TransportError::AuthFailed => {
                eprintlns("ssh: authentication failed");
            }
            _ => {
                print_error("authentication", e);
            }
        }
        close(fd);
        return 1;
    }

    if config.verbose {
        printlns("ssh: authentication successful, opening channel");
    }

    // Open session channel
    let mut channel = match SshChannel::open_session(&mut transport) {
        Ok(c) => c,
        Err(e) => {
            print_error("channel open", e);
            close(fd);
            return 1;
        }
    };

    // Request PTY
    if let Err(e) = channel.request_pty(&mut transport) {
        print_error("pty request", e);
        close(fd);
        return 1;
    }

    // Request shell
    if let Err(e) = channel.request_shell(&mut transport) {
        print_error("shell request", e);
        close(fd);
        return 1;
    }

    if config.verbose {
        printlns("ssh: session established");
    }

    // Run interactive session
    if let Err(e) = run_session(&mut transport, &mut channel) {
        if !matches!(e, TransportError::Closed) {
            print_error("session", e);
        }
    }

    // Close connection
    let _ = channel.close(&mut transport);
    close(fd);

    0
}

/// Parse command line arguments
fn parse_args(argc: i32, argv: *const *const u8) -> Option<SshConfig> {
    let mut config = SshConfig::new();
    let mut i = 1;

    while i < argc {
        let arg = cstr_to_str(unsafe { *argv.add(i as usize) });
        if arg.starts_with("-") {
            match arg {
                "-p" => {
                    i += 1;
                    if i >= argc {
                        return None;
                    }
                    let port_str = cstr_to_str(unsafe { *argv.add(i as usize) });
                    config.port = parse_port(port_str)?;
                }
                "-l" => {
                    i += 1;
                    if i >= argc {
                        return None;
                    }
                    let user_str = cstr_to_str(unsafe { *argv.add(i as usize) });
                    config.username = String::from(user_str);
                }
                "-v" => {
                    config.verbose = true;
                }
                "-o" => {
                    i += 1;
                    if i >= argc {
                        return None;
                    }
                    // Parse option
                    let opt = cstr_to_str(unsafe { *argv.add(i as usize) });
                    if opt.starts_with("StrictHostKeyChecking=no") {
                        config.skip_host_key_check = true;
                    }
                }
                _ => {
                    if arg.starts_with("-o") {
                        // -oOption=value format
                        let opt = &arg[2..];
                        if opt.starts_with("StrictHostKeyChecking=no") {
                            config.skip_host_key_check = true;
                        }
                    } else {
                        // Unknown option
                        return None;
                    }
                }
            }
        } else {
            // hostname or user@hostname
            if let Some(at_pos) = arg.find('@') {
                config.username = String::from(&arg[..at_pos]);
                config.hostname = String::from(&arg[at_pos + 1..]);
            } else {
                config.hostname = String::from(arg);
            }
        }
        i += 1;
    }

    Some(config)
}

/// Parse port number
fn parse_port(s: &str) -> Option<u16> {
    let mut port = 0u16;
    for c in s.chars() {
        if !c.is_ascii_digit() {
            return None;
        }
        port = port.checked_mul(10)?;
        port = port.checked_add((c as u16) - ('0' as u16))?;
    }
    if port == 0 { None } else { Some(port) }
}

/// Resolve hostname to IP address
fn resolve_hostname(hostname: &str) -> Option<u32> {
    // Check if it's already an IP address
    if let Some(ip) = parse_ipv4(hostname) {
        return Some(ip);
    }

    // Try DNS resolution
    if let Some((a, b, c, d)) = dns::resolve(hostname, None) {
        let ip = ((a as u32) << 24) | ((b as u32) << 16) | ((c as u32) << 8) | (d as u32);
        return Some(ip);
    }

    None
}

/// Parse IPv4 address string
fn parse_ipv4(s: &str) -> Option<u32> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 4 {
        return None;
    }

    let mut ip = 0u32;
    for (i, part) in parts.iter().enumerate() {
        let octet = parse_u8(part)?;
        ip |= (octet as u32) << ((3 - i) * 8);
    }
    Some(ip)
}

/// Parse u8 from string
fn parse_u8(s: &str) -> Option<u8> {
    let mut val = 0u16;
    for c in s.chars() {
        if !c.is_ascii_digit() {
            return None;
        }
        val = val.checked_mul(10)?;
        val = val.checked_add((c as u16) - ('0' as u16))?;
    }
    if val > 255 { None } else { Some(val as u8) }
}

/// Connect to SSH server
fn connect_to_server(ip: u32, port: u16) -> Result<i32, &'static str> {
    // Create TCP socket
    let fd = socket(af::INET, sock::STREAM, 0);
    if fd < 0 {
        return Err("failed to create socket");
    }

    // Build sockaddr_in
    let addr = SockAddrIn {
        sin_family: af::INET as u16,
        sin_port: port.to_be(),
        sin_addr: InAddr { s_addr: ip.to_be() },
        sin_zero: [0; 8],
    };

    // Connect
    let result = connect(fd, &addr, SOCKADDR_IN_SIZE);
    if result < 0 {
        close(fd);
        return Err("connection refused");
    }

    Ok(fd)
}

/// Read password from user
fn read_password(username: &str, hostname: &str) -> String {
    // Print prompt
    prints_str(username);
    prints("@");
    prints_str(hostname);
    prints("'s password: ");

    // Read password
    let mut password = String::new();
    let mut buf = [0u8; 1];
    loop {
        let n = read(0, &mut buf);
        if n <= 0 {
            break;
        }
        if buf[0] == b'\n' || buf[0] == b'\r' {
            break;
        }
        if buf[0] == 0x7f || buf[0] == 0x08 {
            // Backspace
            password.pop();
        } else {
            password.push(buf[0] as char);
        }
    }

    printlns(""); // Newline after password

    password
}

/// Get current username
fn get_current_username() -> String {
    // Try to get from environment or /etc/passwd
    let uid = getuid();
    if uid == 0 {
        return String::from("root");
    }

    // Try to read from /etc/passwd
    let fd = open2("/etc/passwd", O_RDONLY);
    if fd >= 0 {
        let mut buf = [0u8; 1024];
        let n = read(fd, &mut buf);
        close(fd);

        if n > 0 {
            // Parse passwd file to find username for uid
            // Format: username:x:uid:gid:...
            let content = core::str::from_utf8(&buf[..n as usize]).unwrap_or("");
            for line in content.lines() {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() >= 3 {
                    if let Some(line_uid) = parse_u32(parts[2]) {
                        if line_uid == uid as u32 {
                            return String::from(parts[0]);
                        }
                    }
                }
            }
        }
    }

    // Default to current user
    String::from("user")
}

/// Parse u32 from string
fn parse_u32(s: &str) -> Option<u32> {
    let mut val = 0u32;
    for c in s.chars() {
        if !c.is_ascii_digit() {
            return None;
        }
        val = val.checked_mul(10)?;
        val = val.checked_add((c as u32) - ('0' as u32))?;
    }
    Some(val)
}

/// Print error message
fn print_error(context: &str, error: TransportError) {
    prints("ssh: ");
    prints(context);
    prints(" failed: ");
    match error {
        TransportError::SendFailed => printlns("send failed"),
        TransportError::RecvFailed => printlns("recv failed"),
        TransportError::Io => printlns("I/O error"),
        TransportError::Protocol => printlns("protocol error"),
        TransportError::InvalidPacket => printlns("invalid packet"),
        TransportError::Decryption => printlns("decryption error"),
        TransportError::KeyExchange => printlns("key exchange error"),
        TransportError::Closed => printlns("connection closed by server"),
        TransportError::HostKeyVerification => printlns("host key verification failed"),
        TransportError::AuthFailed => printlns("authentication failed"),
    }
}

/// Print usage information
fn usage() {
    printlns("Usage: ssh [options] [user@]hostname");
    printlns("");
    printlns("Options:");
    printlns("  -p port    Connect to this port (default: 22)");
    printlns("  -l user    Login as this user");
    printlns("  -v         Verbose mode");
    printlns("  -o option  Set option");
}

/// Helper to print a String
fn prints_str(s: &str) {
    for c in s.chars() {
        let buf = [c as u8];
        let _ = write(1, &buf);
    }
}

fn printlns_str(s: &str) {
    prints_str(s);
    printlns("");
}
