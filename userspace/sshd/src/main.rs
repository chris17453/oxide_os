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

/// Print helper
fn log(msg: &str) {
    prints("[sshd] ");
    prints(msg);
    prints("\n");
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
    log("Starting OXIDE SSH server");

    // Generate or load host key
    if let Err(e) = crypto::init_host_key() {
        log("Failed to initialize host key");
        return 1;
    }
    log("Host key initialized");

    // Create listening socket
    let server_fd = tcp_socket();
    if server_fd < 0 {
        log("Failed to create socket");
        return 1;
    }

    // Set SO_REUSEADDR
    let optval: i32 = 1;
    setsockopt(server_fd, sol::SOCKET, so::REUSEADDR, &optval);

    // Bind to port 22
    let addr = sockaddr_in(SSH_PORT, INADDR_ANY);
    if bind(server_fd, &addr, SOCKADDR_IN_SIZE) < 0 {
        log("Failed to bind to port 22");
        close(server_fd);
        return 1;
    }

    // Listen
    if listen(server_fd, MAX_CONNECTIONS as i32) < 0 {
        log("Failed to listen");
        close(server_fd);
        return 1;
    }

    log("Listening on port 22");

    // Accept loop
    loop {
        let mut client_addr = SockAddrIn::default();
        let mut addr_len = SOCKADDR_IN_SIZE;

        let client_fd = accept(server_fd, Some(&mut client_addr), Some(&mut addr_len));
        if client_fd < 0 {
            log("Accept failed");
            continue;
        }

        // Log client address
        let ip = ntohl(client_addr.sin_addr.s_addr);
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
