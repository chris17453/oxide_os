//! SSH Server Daemon for OXIDE OS
//!
//! Implements SSH-2 protocol (RFC 4253, 4254) with:
//! - curve25519-sha256 key exchange
//! - ssh-ed25519 host keys
//! - chacha20-poly1305@openssh.com encryption
//! - Password authentication

#![no_std]
#![no_main]

extern crate alloc;

mod transport;
mod kex;
mod auth;
mod channel;
mod session;
mod crypto;

use libc::*;
use libc::socket::{
    tcp_socket, sockaddr_in, bind, listen, accept, setsockopt,
    SockAddrIn, SOCKADDR_IN_SIZE, INADDR_ANY, ntohl, ntohs,
    sol, so,
};
use alloc::vec::Vec;

use transport::SshTransport;

/// SSH port
const SSH_PORT: u16 = 22;

/// Maximum connections to handle
const MAX_CONNECTIONS: usize = 10;

/// Log file for debugging (since stdout is redirected to /dev/null by servicemgr)
const LOG_FILE: &str = "/var/log/sshd.log";

/// Print to console (may go to /dev/null)
fn log_console(msg: &str) {
    prints("[sshd] ");
    prints(msg);
    prints("\n");
}

/// Write to log file for persistent debugging
fn log_to_file(msg: &str) {
    let fd = open(LOG_FILE, (O_WRONLY | O_CREAT | O_APPEND) as u32, 0o644);
    if fd >= 0 {
        let prefix = b"[sshd] ";
        let _ = write(fd, prefix);
        let _ = write(fd, msg.as_bytes());
        let _ = write(fd, b"\n");
        close(fd);
    }
}

/// Print helper - writes to both console and log file
fn log(msg: &str) {
    log_console(msg);
    log_to_file(msg);
}

/// Handle a single SSH connection
fn handle_connection(client_fd: i32) {
    log("New connection, starting SSH handshake");

    // Create transport layer
    let mut transport = match SshTransport::new(client_fd) {
        Ok(t) => t,
        Err(e) => {
            log("Failed to create transport");
            return;
        }
    };

    // Protocol version exchange
    if let Err(e) = transport.version_exchange() {
        log("Version exchange failed");
        return;
    }
    log("Version exchange complete");

    // Key exchange
    if let Err(e) = transport.key_exchange() {
        log("Key exchange failed");
        return;
    }
    log("Key exchange complete, encryption enabled");

    // Authentication
    if let Err(e) = auth::authenticate(&mut transport) {
        log("Authentication failed");
        return;
    }
    log("Authentication successful");

    // Handle channels and sessions
    if let Err(e) = session::run_session(&mut transport) {
        log("Session error");
    }

    log("Connection closed");
}

/// Main entry point
#[unsafe(no_mangle)]
pub fn main() -> i32 {
    // Create log directory
    let _ = mkdir("/var", 0o755);
    let _ = mkdir("/var/log", 0o755);

    log("Starting OXIDE SSH server");

    // Generate or load host key
    if let Err(e) = crypto::init_host_key() {
        log("Failed to initialize host key");
        return 1;
    }
    log("Host key initialized");

    // Create listening socket
    log("Creating TCP socket");
    let server_fd = tcp_socket();
    if server_fd < 0 {
        log("Failed to create socket");
        log_to_file("tcp_socket() returned negative fd");
        return 1;
    }
    log_to_file("Socket created successfully");

    // Set SO_REUSEADDR
    let optval: i32 = 1;
    setsockopt(server_fd, sol::SOCKET, so::REUSEADDR, &optval);

    // Bind to port 22
    log("Binding to port 22");
    let addr = sockaddr_in(SSH_PORT, INADDR_ANY);
    let bind_result = bind(server_fd, &addr, SOCKADDR_IN_SIZE);
    if bind_result < 0 {
        log("Failed to bind to port 22");
        close(server_fd);
        return 1;
    }
    log_to_file("Bind successful");

    // Listen
    log("Starting to listen");
    let listen_result = listen(server_fd, MAX_CONNECTIONS as i32);
    if listen_result < 0 {
        log("Failed to listen");
        close(server_fd);
        return 1;
    }

    log("Listening on port 22");
    log_to_file("Now accepting connections on port 22");

    // Accept loop
    log_to_file("Entering accept loop");
    loop {
        let mut client_addr = SockAddrIn::default();
        let mut addr_len = SOCKADDR_IN_SIZE;

        log_to_file("Waiting for connection...");
        let client_fd = accept(server_fd, Some(&mut client_addr), Some(&mut addr_len));
        if client_fd < 0 {
            log("Accept failed");
            log_to_file("accept() returned negative fd");
            continue;
        }

        // Log client address
        let ip = ntohl(client_addr.sin_addr.s_addr);
        log_to_file("Connection accepted");
        prints("[sshd] Connection from ");
        print_i64(((ip >> 24) & 0xff) as i64);
        prints(".");
        print_i64(((ip >> 16) & 0xff) as i64);
        prints(".");
        print_i64(((ip >> 8) & 0xff) as i64);
        prints(".");
        print_i64((ip & 0xff) as i64);
        prints(":");
        print_i64(ntohs(client_addr.sin_port) as i64);
        prints("\n");

        // Fork to handle connection
        let pid = fork();
        if pid < 0 {
            log("Fork failed");
            close(client_fd);
            continue;
        }

        if pid == 0 {
            // Child process - handle connection
            close(server_fd);
            handle_connection(client_fd);
            close(client_fd);
            exit(0);
        } else {
            // Parent - continue accepting
            close(client_fd);
        }
    }
}
