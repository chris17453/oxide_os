//! whoami - print effective user name
//!
//! Enhanced implementation with:
//! - /etc/passwd lookup for username
//! - Falls back to UID if username not found
//! - Proper passwd file parsing

#![no_std]
#![no_main]

use libc::*;

const PASSWD_PATH: &str = "/etc/passwd";
const MAX_LINE: usize = 512;

/// Parse a passwd line and return (username, uid) if valid
fn parse_passwd_line(line: &[u8]) -> Option<(&[u8], u32)> {
    // Format: username:password:uid:gid:gecos:home:shell
    // We only care about username and uid

    if line.is_empty() || line[0] == b'#' {
        return None;
    }

    // Find first colon (end of username)
    let first_colon = line.iter().position(|&b| b == b':')?;
    let username = &line[..first_colon];

    // Find second colon (skip password field)
    let second_colon = line[first_colon + 1..].iter().position(|&b| b == b':')?;
    let second_colon = first_colon + 1 + second_colon;

    // Find third colon (end of uid)
    let third_colon = line[second_colon + 1..].iter().position(|&b| b == b':')?;
    let third_colon = second_colon + 1 + third_colon;

    // Parse UID
    let uid_bytes = &line[second_colon + 1..third_colon];
    let mut uid = 0u32;
    for &b in uid_bytes {
        if b >= b'0' && b <= b'9' {
            uid = uid * 10 + (b - b'0') as u32;
        } else {
            return None;
        }
    }

    Some((username, uid))
}

/// Look up username for given UID from /etc/passwd
fn get_username(target_uid: u32) -> Option<[u8; 32]> {
    // Open /etc/passwd
    let fd = open2(PASSWD_PATH, O_RDONLY);
    if fd < 0 {
        return None;
    }

    let mut buf = [0u8; 4096];
    let mut line_buf = [0u8; MAX_LINE];
    let mut line_len = 0;
    let mut result: Option<[u8; 32]> = None;

    loop {
        let n = read(fd, &mut buf);
        if n <= 0 {
            break;
        }

        for i in 0..(n as usize) {
            let byte = buf[i];

            if byte == b'\n' {
                // Process complete line
                if line_len > 0 {
                    if let Some((username, uid)) = parse_passwd_line(&line_buf[..line_len]) {
                        if uid == target_uid {
                            // Found matching entry
                            let mut name = [0u8; 32];
                            let copy_len = if username.len() > 31 { 31 } else { username.len() };
                            name[..copy_len].copy_from_slice(&username[..copy_len]);
                            result = Some(name);
                            break;
                        }
                    }
                }
                line_len = 0;
            } else if line_len < MAX_LINE {
                line_buf[line_len] = byte;
                line_len += 1;
            }
        }

        if result.is_some() {
            break;
        }
    }

    // Process last line if no newline at end
    if result.is_none() && line_len > 0 {
        if let Some((username, uid)) = parse_passwd_line(&line_buf[..line_len]) {
            if uid == target_uid {
                let mut name = [0u8; 32];
                let copy_len = if username.len() > 31 { 31 } else { username.len() };
                name[..copy_len].copy_from_slice(&username[..copy_len]);
                result = Some(name);
            }
        }
    }

    close(fd);
    result
}

#[unsafe(no_mangle)]
fn main() -> i32 {
    let euid = geteuid();

    // Try to look up username from /etc/passwd
    if let Some(name_buf) = get_username(euid) {
        // Find null terminator
        let len = name_buf.iter().position(|&b| b == 0).unwrap_or(32);

        // Print username
        for i in 0..len {
            putchar(name_buf[i]);
        }
        printlns("");
    } else {
        // Fall back to numeric UID if not found
        prints("uid=");
        print_u64(euid as u64);
        printlns("");
    }

    0
}
