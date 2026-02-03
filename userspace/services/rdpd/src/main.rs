//! RDP Server Daemon for OXIDE OS
//!
//! Implements RDP (Remote Desktop Protocol) server:
//! - Listens on port 3389 (standard RDP port)
//! - Provides remote desktop access to OXIDE OS
//! - Integrates with kernel RDP subsystem
//! - Screen sharing via framebuffer
//! - Keyboard/mouse input forwarding
//!
//! Config: /etc/rdpd.conf
//! Format:
//!   port=3389
//!   max_connections=10
//!   tls_required=yes
//!   log_level=info

#![no_std]
#![no_main]
#![allow(unused)]

extern crate alloc;

use libc::poll::{PollFd, events::POLLIN, poll};
use libc::socket::{
    INADDR_ANY, SOCKADDR_IN_SIZE, SockAddrIn, accept, bind, listen, setsockopt, so, sockaddr_in,
    sol, tcp_socket,
};
use libc::*;

/// RDP port (standard)
const RDP_PORT: u16 = 3389;

/// Maximum connections to handle
const MAX_CONNECTIONS: usize = 10;

/// Log file for debugging
const LOG_FILE: &str = "/var/log/rdpd.log";

/// Config file
const CONFIG_FILE: &str = "/etc/rdpd.conf";

/// Server configuration
struct ServerConfig {
    port: u16,
    max_connections: usize,
    tls_required: bool,
}

impl ServerConfig {
    fn default() -> Self {
        Self {
            port: RDP_PORT,
            max_connections: MAX_CONNECTIONS,
            tls_required: true,
        }
    }

    fn load() -> Self {
        let mut config = Self::default();
        
        // Try to load from config file
        let fd = open(CONFIG_FILE, O_RDONLY as u32, 0);
        if fd >= 0 {
            let mut buf = [0u8; 1024];
            let n = read(fd, &mut buf);
            close(fd);
            
            if n > 0 {
                let content = core::str::from_utf8(&buf[..n as usize]).unwrap_or("");
                for line in content.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    
                    if let Some((key, value)) = line.split_once('=') {
                        let key = key.trim();
                        let value = value.trim();
                        
                        match key {
                            "port" => {
                                if let Ok(p) = value.parse::<u16>() {
                                    config.port = p;
                                }
                            }
                            "max_connections" => {
                                if let Ok(m) = value.parse::<usize>() {
                                    config.max_connections = m;
                                }
                            }
                            "tls_required" => {
                                config.tls_required = value == "yes" || value == "true";
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        
        config
    }
}

/// Print to console
fn log_console(msg: &str) {
    prints("[rdpd] ");
    prints(msg);
    prints("\n");
}

/// Write to log file for persistent debugging
fn log_to_file(msg: &str) {
    let fd = open(LOG_FILE, (O_WRONLY | O_CREAT | O_APPEND) as u32, 0o644);
    if fd >= 0 {
        let prefix = b"[rdpd] ";
        let _ = write(fd, prefix);
        let _ = write(fd, msg.as_bytes());
        let _ = write(fd, b"\n");
        close(fd);
    }
}

/// Print helper - writes to both console and log file
/// — ShadePacket: Network daemon logging matrix, dual-stream for reliability
fn log(msg: &str) {
    log_console(msg);
    log_to_file(msg);
}

/// Handle a single RDP connection
/// — GlassSignal: Client connection handler, bridge to framebuffer realm
fn handle_connection(client_fd: i32) {
    log("New RDP connection established");
    
    // TODO: Implement full RDP protocol handshake
    // For now, we'll just log and close
    // In a full implementation, this would:
    // 1. Handle TLS handshake
    // 2. Parse RDP connection sequence
    // 3. Set up virtual channels
    // 4. Start screen capture and input forwarding
    // 5. Enter main loop for data transfer
    
    let response = b"RDP/1.0 OXIDE OS RDP Server\r\n";
    let _ = write(client_fd, response);
    
    // For now, just keep the connection open briefly
    // In production, this would handle the full RDP session
    let mut buf = [0u8; 4096];
    loop {
        let n = read(client_fd, &mut buf);
        if n <= 0 {
            break;
        }
        log("Received RDP data");
        // Process RDP packets here
    }
    
    log("RDP connection closed");
}

/// Main daemon entry point
/// — NeonRoot: RDP service bootstrap, system-wide remote desktop gateway
#[unsafe(no_mangle)]
pub fn main() -> i32 {
    log("RDP daemon starting...");
    
    // Load configuration
    let config = ServerConfig::load();
    
    // Log configuration
    let mut port_str = [0u8; 64];
    let port_len = format_number(config.port as u32, &mut port_str);
    let port_msg = core::str::from_utf8(&port_str[..port_len]).unwrap_or("?");
    
    log_console("Configuration:");
    prints("  Port: ");
    prints(port_msg);
    prints("\n");
    
    let mut max_str = [0u8; 64];
    let max_len = format_number(config.max_connections as u32, &mut max_str);
    let max_msg = core::str::from_utf8(&max_str[..max_len]).unwrap_or("?");
    prints("  Max connections: ");
    prints(max_msg);
    prints("\n");
    
    prints("  TLS required: ");
    prints(if config.tls_required { "yes" } else { "no" });
    prints("\n");
    
    // Check if we're running as the correct user (rdp user, uid 75)
    let uid = getuid();
    let gid = getgid();
    
    // For now, allow running as any user during development
    // In production, enforce uid 75
    if uid != 75 {
        log("Warning: Not running as rdp user (uid 75)");
    }
    
    // Create socket
    let server_fd = tcp_socket();
    if server_fd < 0 {
        eprintlns("rdpd: Failed to create socket");
        return 1;
    }
    
    // Set SO_REUSEADDR to allow quick restart
    let optval: i32 = 1;
    let _ = setsockopt(
        server_fd,
        sol::SOCKET,
        so::REUSEADDR,
        &optval,
    );
    
    // Bind to port
    let addr = sockaddr_in(config.port, INADDR_ANY);
    if bind(server_fd, &addr, SOCKADDR_IN_SIZE) < 0 {
        eprintlns("rdpd: Failed to bind to port");
        close(server_fd);
        return 1;
    }
    
    log("Bound to RDP port");
    
    // Listen for connections
    if listen(server_fd, config.max_connections as i32) < 0 {
        eprintlns("rdpd: Failed to listen");
        close(server_fd);
        return 1;
    }
    
    log("RDP server listening and ready");
    
    // Main accept loop
    loop {
        // Use poll to check for incoming connections
        let mut fds = [PollFd {
            fd: server_fd,
            events: POLLIN,
            revents: 0,
        }];
        
        // Wait for connection with 5 second timeout
        let ready = poll(&mut fds, 5000);
        if ready < 0 {
            log("Poll error");
            break;
        }
        
        if ready == 0 {
            // Timeout, continue
            continue;
        }
        
        // Accept connection
        let mut client_addr: SockAddrIn = unsafe { core::mem::zeroed() };
        let mut addr_len = SOCKADDR_IN_SIZE;
        
        let client_fd = accept(server_fd, Some(&mut client_addr), Some(&mut addr_len));
        
        if client_fd < 0 {
            log("Accept failed");
            continue;
        }
        
        log("Accepted new connection");
        
        // Handle connection
        // In production, fork or create thread here
        handle_connection(client_fd);
        close(client_fd);
    }
    
    close(server_fd);
    log("RDP daemon shutting down");
    0
}

/// Helper to format a number as string
/// — Hexline: Number-to-ASCII serialization primitive
fn format_number(mut n: u32, buf: &mut [u8]) -> usize {
    if n == 0 {
        buf[0] = b'0';
        return 1;
    }
    
    let mut i = 0;
    let mut temp = [0u8; 20];
    let mut temp_i = 0;
    
    while n > 0 {
        temp[temp_i] = b'0' + (n % 10) as u8;
        temp_i += 1;
        n /= 10;
    }
    
    // Reverse into output buffer
    while temp_i > 0 {
        temp_i -= 1;
        buf[i] = temp[temp_i];
        i += 1;
    }
    
    i
}
