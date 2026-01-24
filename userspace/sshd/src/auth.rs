//! SSH User Authentication (RFC 4252)
//!
//! Implements password authentication.

use alloc::string::String;
use alloc::vec::Vec;
use core::cell::UnsafeCell;
use libc::*;

use crate::transport::{
    SshTransport, TransportError, TransportResult, decode_string, decode_u8, encode_string, msg,
};

/// Maximum authentication attempts
const MAX_AUTH_ATTEMPTS: u32 = 3;

/// Authenticated user info
pub struct AuthenticatedUser {
    pub username: String,
    pub uid: u32,
    pub gid: u32,
    pub home: String,
    pub shell: String,
}

/// Thread-safe cell wrapper
struct SyncUnsafeCell<T>(UnsafeCell<T>);
unsafe impl<T> Sync for SyncUnsafeCell<T> {}

impl<T> SyncUnsafeCell<T> {
    const fn new(value: T) -> Self {
        SyncUnsafeCell(UnsafeCell::new(value))
    }

    fn get(&self) -> *mut T {
        self.0.get()
    }
}

static AUTHENTICATED_USER: SyncUnsafeCell<Option<AuthenticatedUser>> = SyncUnsafeCell::new(None);

/// Get authenticated user (after successful auth)
pub fn authenticated_user() -> Option<&'static AuthenticatedUser> {
    unsafe { (*AUTHENTICATED_USER.get()).as_ref() }
}

/// Perform user authentication
pub fn authenticate(transport: &mut SshTransport) -> TransportResult<()> {
    // Wait for service request
    let service_req = transport.recv_packet()?;
    if service_req.is_empty() || service_req[0] != msg::SERVICE_REQUEST {
        return Err(TransportError::Protocol);
    }

    // Parse service name
    let mut offset = 1;
    let service = decode_string(&service_req, &mut offset)?;
    if &service != b"ssh-userauth" {
        return Err(TransportError::Protocol);
    }

    // Accept service request
    let mut accept = Vec::with_capacity(20);
    accept.push(msg::SERVICE_ACCEPT);
    accept.extend_from_slice(&encode_string(b"ssh-userauth"));
    transport.send_packet(&accept)?;

    // Authentication loop
    let mut attempts = 0u32;

    loop {
        let auth_req = transport.recv_packet()?;
        if auth_req.is_empty() || auth_req[0] != msg::USERAUTH_REQUEST {
            return Err(TransportError::Protocol);
        }

        let mut offset = 1;
        let username = decode_string(&auth_req, &mut offset)?;
        let service = decode_string(&auth_req, &mut offset)?;
        let method = decode_string(&auth_req, &mut offset)?;

        // Verify service is "ssh-connection"
        if &service != b"ssh-connection" {
            send_auth_failure(transport, false)?;
            continue;
        }

        // Handle authentication method
        match method.as_slice() {
            b"none" => {
                // None method - always fails but tells client what methods we support
                send_auth_failure(transport, false)?;
            }
            b"password" => {
                // Password authentication
                let change_password = decode_u8(&auth_req, &mut offset)?;
                if change_password != 0 {
                    // Password change not supported
                    send_auth_failure(transport, false)?;
                    continue;
                }

                let password = decode_string(&auth_req, &mut offset)?;

                // Verify password
                let username_str = core::str::from_utf8(&username).unwrap_or("");
                let password_str = core::str::from_utf8(&password).unwrap_or("");

                if let Some(user) = verify_password(username_str, password_str) {
                    // Success!
                    unsafe {
                        *AUTHENTICATED_USER.get() = Some(user);
                    }
                    transport.send_packet(&[msg::USERAUTH_SUCCESS])?;
                    return Ok(());
                }

                attempts += 1;
                if attempts >= MAX_AUTH_ATTEMPTS {
                    return Err(TransportError::Protocol);
                }
                send_auth_failure(transport, false)?;
            }
            b"publickey" => {
                // Public key authentication - not implemented yet
                send_auth_failure(transport, false)?;
            }
            _ => {
                // Unknown method
                send_auth_failure(transport, false)?;
            }
        }
    }
}

/// Send authentication failure message
fn send_auth_failure(transport: &mut SshTransport, partial: bool) -> TransportResult<()> {
    let mut msg = Vec::with_capacity(32);
    msg.push(msg::USERAUTH_FAILURE);

    // Methods that can continue
    msg.extend_from_slice(&encode_string(b"password"));

    // Partial success
    msg.push(if partial { 1 } else { 0 });

    transport.send_packet(&msg)
}

/// Verify password against /etc/passwd
fn verify_password(username: &str, password: &str) -> Option<AuthenticatedUser> {
    // Try to read /etc/passwd
    let fd = open2("/etc/passwd", O_RDONLY);
    if fd >= 0 {
        let mut buf = [0u8; 2048];
        let n = read(fd, &mut buf);
        close(fd);

        if n > 0 {
            let content = &buf[..n as usize];
            if let Some(user) = parse_passwd(content, username, password) {
                return Some(user);
            }
        }
    }

    // Fallback to built-in users
    verify_builtin_password(username, password)
}

/// Parse /etc/passwd format: username:password:uid:gid:gecos:home:shell
fn parse_passwd(content: &[u8], username: &str, password: &str) -> Option<AuthenticatedUser> {
    let username_bytes = username.as_bytes();
    let password_bytes = password.as_bytes();

    let mut line_start = 0;
    while line_start < content.len() {
        // Find end of line
        let mut line_end = line_start;
        while line_end < content.len() && content[line_end] != b'\n' {
            line_end += 1;
        }

        let line = &content[line_start..line_end];

        // Parse line
        if let Some(user) = parse_passwd_line(line, username_bytes, password_bytes) {
            return Some(user);
        }

        line_start = line_end + 1;
    }

    None
}

fn parse_passwd_line(line: &[u8], username: &[u8], password: &[u8]) -> Option<AuthenticatedUser> {
    let fields: Vec<&[u8]> = line.split(|&c| c == b':').collect();
    if fields.len() < 7 {
        return None;
    }

    // Check username
    if fields[0] != username {
        return None;
    }

    // Check password (field 1)
    if fields[1] != password {
        return None;
    }

    // Parse UID and GID
    let uid = parse_number(fields[2])?;
    let gid = parse_number(fields[3])?;

    // Home and shell
    let home = core::str::from_utf8(fields[5]).ok()?;
    let shell = core::str::from_utf8(fields[6]).ok()?;
    let username_str = core::str::from_utf8(username).ok()?;

    Some(AuthenticatedUser {
        username: String::from(username_str),
        uid,
        gid,
        home: String::from(home),
        shell: String::from(shell),
    })
}

fn parse_number(s: &[u8]) -> Option<u32> {
    let mut result = 0u32;
    for &c in s {
        if c < b'0' || c > b'9' {
            return None;
        }
        result = result.checked_mul(10)?.checked_add((c - b'0') as u32)?;
    }
    Some(result)
}

/// Built-in user database
fn verify_builtin_password(username: &str, password: &str) -> Option<AuthenticatedUser> {
    match username {
        "root" => {
            if password == "root" || password.is_empty() {
                return Some(AuthenticatedUser {
                    username: String::from("root"),
                    uid: 0,
                    gid: 0,
                    home: String::from("/root"),
                    shell: String::from("/bin/esh"),
                });
            }
        }
        "user" => {
            if password == "user" {
                return Some(AuthenticatedUser {
                    username: String::from("user"),
                    uid: 1000,
                    gid: 1000,
                    home: String::from("/home/user"),
                    shell: String::from("/bin/esh"),
                });
            }
        }
        _ => {}
    }
    None
}
