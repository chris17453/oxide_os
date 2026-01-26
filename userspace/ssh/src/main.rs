//! OXIDE SSH Client
//!
//! A clean SSH client implementation using oxide-std.
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
#![allow(unused)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use oxide_std::io::Write;
use oxide_std::net::TcpStream;
use oxide_std::{eprintln, print, println};

mod crypto;
mod kex;
mod session;
mod transport;

use kex::perform_key_exchange;
use session::{SshChannel, authenticate_password, request_userauth_service, run_session};
use transport::{SshTransport, TransportError};

/// SSH client configuration
struct Config {
    hostname: String,
    port: u16,
    username: String,
    verbose: bool,
    skip_host_key_check: bool,
}

impl Config {
    fn new() -> Self {
        Config {
            hostname: String::new(),
            port: 22,
            username: String::new(),
            verbose: false,
            skip_host_key_check: false,
        }
    }
}

/// Main entry point
#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        print_usage();
        return 1;
    }

    let config = match parse_args(argc, argv) {
        Some(c) => c,
        None => {
            print_usage();
            return 1;
        }
    };

    if config.hostname.is_empty() {
        eprintln!("ssh: missing hostname");
        return 1;
    }

    // Get username if not specified
    let username = if config.username.is_empty() {
        get_current_username()
    } else {
        config.username.clone()
    };

    if config.verbose {
        println!("ssh: connecting to {}:{}", config.hostname, config.port);
    }

    // Connect to server
    let stream = match TcpStream::connect_host(&config.hostname, config.port) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("ssh: connection failed: {:?}", e);
            return 1;
        }
    };

    if config.verbose {
        println!("ssh: connected, starting SSH handshake");
    }

    // Create transport
    let mut transport = SshTransport::new(stream);

    // Version exchange
    if config.verbose {
        println!("ssh: version exchange");
    }
    if let Err(e) = transport.version_exchange() {
        print_error("version exchange", e);
        return 1;
    }

    // Key exchange
    if config.verbose {
        println!("ssh: key exchange");
    }
    if let Err(e) = perform_key_exchange(&mut transport) {
        print_error("key exchange", e);
        return 1;
    }

    if config.verbose {
        println!("ssh: key exchange complete, requesting authentication");
    }

    // Request userauth service
    if let Err(e) = request_userauth_service(&mut transport) {
        print_error("service request", e);
        return 1;
    }

    // Get password
    let password = read_password(&username, &config.hostname);

    // Authenticate
    if config.verbose {
        println!("ssh: authenticating");
    }
    if let Err(e) = authenticate_password(&mut transport, username.as_bytes(), password.as_bytes())
    {
        match e {
            TransportError::AuthFailed => eprintln!("ssh: authentication failed"),
            _ => print_error("authentication", e),
        }
        return 1;
    }

    if config.verbose {
        println!("ssh: authentication successful, opening channel");
    }

    // Open session channel
    let mut channel = match SshChannel::open_session(&mut transport) {
        Ok(c) => c,
        Err(e) => {
            print_error("channel open", e);
            return 1;
        }
    };

    // Request PTY
    if let Err(e) = channel.request_pty(&mut transport) {
        print_error("pty request", e);
        return 1;
    }

    // Request shell
    if let Err(e) = channel.request_shell(&mut transport) {
        print_error("shell request", e);
        return 1;
    }

    if config.verbose {
        println!("ssh: session established");
    }

    // Run interactive session
    if let Err(e) = run_session(&mut transport, &mut channel) {
        if !matches!(e, TransportError::Closed) {
            print_error("session", e);
        }
    }

    // Close connection
    let _ = channel.close(&mut transport);

    0
}

/// Parse command line arguments
fn parse_args(argc: i32, argv: *const *const u8) -> Option<Config> {
    let mut config = Config::new();
    let mut i = 1;

    while i < argc {
        let arg = cstr_to_str(unsafe { *argv.add(i as usize) });
        if arg.starts_with('-') {
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
                    let opt = cstr_to_str(unsafe { *argv.add(i as usize) });
                    if opt.starts_with("StrictHostKeyChecking=no") {
                        config.skip_host_key_check = true;
                    }
                }
                _ => {
                    if arg.starts_with("-o") {
                        let opt = &arg[2..];
                        if opt.starts_with("StrictHostKeyChecking=no") {
                            config.skip_host_key_check = true;
                        }
                    } else {
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

/// Read password from user
fn read_password(username: &str, hostname: &str) -> String {
    print!("{}@{}'s password: ", username, hostname);

    let mut password = String::new();
    let mut buf = [0u8; 1];
    loop {
        let n = libc::read(0, &mut buf);
        if n <= 0 {
            break;
        }
        if buf[0] == b'\n' || buf[0] == b'\r' {
            break;
        }
        if buf[0] == 0x7f || buf[0] == 0x08 {
            password.pop();
        } else {
            password.push(buf[0] as char);
        }
    }

    println!("");
    password
}

/// Get current username
fn get_current_username() -> String {
    let uid = libc::getuid();
    if uid == 0 {
        return String::from("root");
    }

    // Try to read from /etc/passwd
    let fd = libc::open2("/etc/passwd", libc::O_RDONLY);
    if fd >= 0 {
        let mut buf = [0u8; 1024];
        let n = libc::read(fd, &mut buf);
        libc::close(fd);

        if n > 0 {
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
    let msg = match error {
        TransportError::Io(_) => "I/O error",
        TransportError::Protocol => "protocol error",
        TransportError::InvalidPacket => "invalid packet",
        TransportError::Decryption => "decryption error",
        TransportError::KeyExchange => "key exchange error",
        TransportError::Closed => "connection closed",
        TransportError::HostKeyVerification => "host key verification failed",
        TransportError::AuthFailed => "authentication failed",
    };
    eprintln!("ssh: {} failed: {}", context, msg);
}

/// Print usage information
fn print_usage() {
    println!("Usage: ssh [options] [user@]hostname");
    println!("");
    println!("Options:");
    println!("  -p port    Connect to this port (default: 22)");
    println!("  -l user    Login as this user");
    println!("  -v         Verbose mode");
    println!("  -o option  Set option");
}
