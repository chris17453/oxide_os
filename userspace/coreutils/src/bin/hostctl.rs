//! hostctl - Manage /etc/hosts file entries
//!
//! Utility for managing local hostname-to-IP mappings in /etc/hosts.
//! Persona: ColdCipher - Security & name resolution specialist
//!
//! Commands:
//! - hostctl add <hostname> <ip>     : Add or update a hostname entry
//! - hostctl remove <hostname>       : Remove a hostname entry
//! - hostctl list                    : List all hostname entries
//! - hostctl lookup <hostname>       : Look up an IP for a hostname
//! - hostctl clear                   : Clear all non-localhost entries

#![no_std]
#![no_main]

use libc::*;
use libc::c_exports::link;

const HOSTS_FILE: &str = "/etc/hosts";
const HOSTS_BACKUP: &str = "/etc/hosts.bak";
const MAX_FILE_SIZE: usize = 4096;

/// Print usage information
fn print_usage() {
    printlns("hostctl - Manage /etc/hosts file entries");
    printlns("");
    printlns("Usage:");
    printlns("  hostctl add <hostname> <ip>     Add or update hostname");
    printlns("  hostctl remove <hostname>       Remove hostname");
    printlns("  hostctl list                    List all entries");
    printlns("  hostctl lookup <hostname>       Look up IP address");
    printlns("  hostctl clear                   Clear non-localhost entries");
    printlns("");
    printlns("Examples:");
    printlns("  hostctl add myserver 192.168.1.100");
    printlns("  hostctl remove myserver");
    printlns("  hostctl list");
    printlns("  hostctl lookup myserver");
}

/// Check if IP address is valid
fn is_valid_ip(ip: &str) -> bool {
    let mut octets = 0;
    let mut current = 0u16;
    let mut has_digit = false;

    for c in ip.bytes() {
        if c == b'.' {
            if !has_digit || current > 255 {
                return false;
            }
            octets += 1;
            current = 0;
            has_digit = false;
        } else if c >= b'0' && c <= b'9' {
            current = current * 10 + (c - b'0') as u16;
            has_digit = true;
            if current > 255 {
                return false;
            }
        } else {
            return false;
        }
    }

    octets == 3 && has_digit && current <= 255
}

/// Check if hostname is valid
fn is_valid_hostname(hostname: &str) -> bool {
    if hostname.is_empty() || hostname.len() > 253 {
        return false;
    }

    for c in hostname.bytes() {
        if !(c.is_ascii_alphanumeric() || c == b'.' || c == b'-' || c == b'_') {
            return false;
        }
    }

    // ColdCipher: Don't allow entries for localhost to be modified
    if hostname == "localhost" {
        return false;
    }

    true
}

/// Read /etc/hosts into buffer
fn read_hosts_file(buf: &mut [u8]) -> isize {
    let fd = open2(HOSTS_FILE, O_RDONLY);
    if fd < 0 {
        return 0; // File doesn't exist yet
    }

    let n = read(fd, buf);
    close(fd);
    n
}

/// Write buffer to /etc/hosts
fn write_hosts_file(buf: &[u8], len: usize) -> bool {
    // ColdCipher: Create backup before modifying
    let _ = unlink(HOSTS_BACKUP);
    let _ = link(HOSTS_FILE, HOSTS_BACKUP);

    let fd = open(HOSTS_FILE, (O_WRONLY | O_CREAT | O_TRUNC) as u32, 0o644);
    if fd < 0 {
        eprintlns("Error: Failed to open /etc/hosts for writing");
        return false;
    }

    let written = write(fd, &buf[..len]);
    close(fd);

    written == len as isize
}

/// Add or update hostname entry
fn cmd_add(hostname: &str, ip: &str) -> i32 {
    if !is_valid_hostname(hostname) {
        eprintlns("Error: Invalid hostname");
        return 1;
    }

    if !is_valid_ip(ip) {
        eprintlns("Error: Invalid IP address");
        return 1;
    }

    let mut buf = [0u8; MAX_FILE_SIZE];
    let n = read_hosts_file(&mut buf);

    if n < 0 {
        eprintlns("Error: Failed to read /etc/hosts");
        return 1;
    }

    let mut new_buf = [0u8; MAX_FILE_SIZE];
    let mut new_len = 0;
    let mut found = false;

    // ColdCipher: Parse existing entries, updating matching hostname
    if n > 0 {
        let content = core::str::from_utf8(&buf[..n as usize]).unwrap_or("");

        for line in content.lines() {
            let line = line.trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                if new_len + line.len() + 1 < MAX_FILE_SIZE {
                    new_buf[new_len..new_len + line.len()].copy_from_slice(line.as_bytes());
                    new_len += line.len();
                    new_buf[new_len] = b'\n';
                    new_len += 1;
                }
                continue;
            }

            // Parse IP and hostnames
            let mut parts = line.split_whitespace();
            if let Some(_line_ip) = parts.next() {
                let mut has_hostname = false;
                for name in parts {
                    if name.eq_ignore_ascii_case(hostname) {
                        has_hostname = true;
                        found = true;
                        break;
                    }
                }

                // If this line contains our hostname, replace with new IP
                if has_hostname {
                    if new_len + ip.len() + hostname.len() + 2 < MAX_FILE_SIZE {
                        new_buf[new_len..new_len + ip.len()].copy_from_slice(ip.as_bytes());
                        new_len += ip.len();
                        new_buf[new_len] = b' ';
                        new_len += 1;
                        new_buf[new_len..new_len + hostname.len()]
                            .copy_from_slice(hostname.as_bytes());
                        new_len += hostname.len();
                        new_buf[new_len] = b'\n';
                        new_len += 1;
                    }
                } else {
                    // Keep original line
                    if new_len + line.len() + 1 < MAX_FILE_SIZE {
                        new_buf[new_len..new_len + line.len()].copy_from_slice(line.as_bytes());
                        new_len += line.len();
                        new_buf[new_len] = b'\n';
                        new_len += 1;
                    }
                }
            }
        }
    }

    // If hostname wasn't found, append new entry
    if !found {
        if new_len + ip.len() + hostname.len() + 2 < MAX_FILE_SIZE {
            new_buf[new_len..new_len + ip.len()].copy_from_slice(ip.as_bytes());
            new_len += ip.len();
            new_buf[new_len] = b' ';
            new_len += 1;
            new_buf[new_len..new_len + hostname.len()].copy_from_slice(hostname.as_bytes());
            new_len += hostname.len();
            new_buf[new_len] = b'\n';
            new_len += 1;
        }
    }

    if write_hosts_file(&new_buf, new_len) {
        prints("Added: ");
        prints(hostname);
        prints(" -> ");
        prints(ip);
        prints("\n");
        0
    } else {
        eprintlns("Error: Failed to write /etc/hosts");
        1
    }
}

/// Remove hostname entry
fn cmd_remove(hostname: &str) -> i32 {
    if !is_valid_hostname(hostname) {
        eprintlns("Error: Invalid hostname");
        return 1;
    }

    let mut buf = [0u8; MAX_FILE_SIZE];
    let n = read_hosts_file(&mut buf);

    if n <= 0 {
        eprintlns("Error: /etc/hosts is empty or cannot be read");
        return 1;
    }

    let mut new_buf = [0u8; MAX_FILE_SIZE];
    let mut new_len = 0;
    let mut found = false;

    let content = core::str::from_utf8(&buf[..n as usize]).unwrap_or("");

    for line in content.lines() {
        let line = line.trim();

        // Keep empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            if new_len + line.len() + 1 < MAX_FILE_SIZE {
                new_buf[new_len..new_len + line.len()].copy_from_slice(line.as_bytes());
                new_len += line.len();
                new_buf[new_len] = b'\n';
                new_len += 1;
            }
            continue;
        }

        // Check if this line contains the hostname
        let mut parts = line.split_whitespace();
        if let Some(_ip) = parts.next() {
            let mut has_hostname = false;
            for name in parts {
                if name.eq_ignore_ascii_case(hostname) {
                    has_hostname = true;
                    found = true;
                    break;
                }
            }

            // Skip this line if it contains our hostname
            if !has_hostname {
                if new_len + line.len() + 1 < MAX_FILE_SIZE {
                    new_buf[new_len..new_len + line.len()].copy_from_slice(line.as_bytes());
                    new_len += line.len();
                    new_buf[new_len] = b'\n';
                    new_len += 1;
                }
            }
        }
    }

    if !found {
        prints("Hostname ");
        prints(hostname);
        printlns(" not found");
        return 1;
    }

    if write_hosts_file(&new_buf, new_len) {
        prints("Removed: ");
        prints(hostname);
        prints("\n");
        0
    } else {
        eprintlns("Error: Failed to write /etc/hosts");
        1
    }
}

/// List all entries
fn cmd_list() -> i32 {
    let mut buf = [0u8; MAX_FILE_SIZE];
    let n = read_hosts_file(&mut buf);

    if n <= 0 {
        printlns("# /etc/hosts is empty");
        return 0;
    }

    let content = core::str::from_utf8(&buf[..n as usize]).unwrap_or("");

    // ColdCipher: Display in formatted table
    printlns("IP Address        Hostname(s)");
    printlns("================ ============");

    for line in content.lines() {
        let line = line.trim();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let mut parts = line.split_whitespace();
        if let Some(ip) = parts.next() {
            // Print IP with padding
            prints(ip);
            for _ in ip.len()..17 {
                prints(" ");
            }

            // Print hostnames
            for (i, name) in parts.enumerate() {
                if i > 0 {
                    prints(", ");
                }
                prints(name);
            }
            prints("\n");
        }
    }

    0
}

/// Look up hostname
fn cmd_lookup(hostname: &str) -> i32 {
    if !is_valid_hostname(hostname) {
        eprintlns("Error: Invalid hostname");
        return 1;
    }

    let mut buf = [0u8; MAX_FILE_SIZE];
    let n = read_hosts_file(&mut buf);

    if n <= 0 {
        prints("Hostname ");
        prints(hostname);
        printlns(" not found");
        return 1;
    }

    let content = core::str::from_utf8(&buf[..n as usize]).unwrap_or("");

    for line in content.lines() {
        let line = line.trim();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let mut parts = line.split_whitespace();
        if let Some(ip) = parts.next() {
            for name in parts {
                if name.eq_ignore_ascii_case(hostname) {
                    prints(hostname);
                    prints(" -> ");
                    prints(ip);
                    prints("\n");
                    return 0;
                }
            }
        }
    }

    prints("Hostname ");
    prints(hostname);
    printlns(" not found");
    1
}

/// Clear all non-localhost entries
fn cmd_clear() -> i32 {
    let mut buf = [0u8; MAX_FILE_SIZE];
    let n = read_hosts_file(&mut buf);

    let mut new_buf = [0u8; MAX_FILE_SIZE];
    let mut new_len = 0;

    if n > 0 {
        let content = core::str::from_utf8(&buf[..n as usize]).unwrap_or("");

        for line in content.lines() {
            let line = line.trim();

            // Keep comments and empty lines
            if line.is_empty() || line.starts_with('#') {
                if new_len + line.len() + 1 < MAX_FILE_SIZE {
                    new_buf[new_len..new_len + line.len()].copy_from_slice(line.as_bytes());
                    new_len += line.len();
                    new_buf[new_len] = b'\n';
                    new_len += 1;
                }
                continue;
            }

            // Keep localhost entries
            if line.contains("localhost") || line.starts_with("127.") || line.starts_with("::1") {
                if new_len + line.len() + 1 < MAX_FILE_SIZE {
                    new_buf[new_len..new_len + line.len()].copy_from_slice(line.as_bytes());
                    new_len += line.len();
                    new_buf[new_len] = b'\n';
                    new_len += 1;
                }
            }
        }
    }

    // ColdCipher: If no localhost entries, add defaults
    if new_len == 0 {
        let default = b"127.0.0.1 localhost\n::1 localhost\n";
        new_buf[..default.len()].copy_from_slice(default);
        new_len = default.len();
    }

    if write_hosts_file(&new_buf, new_len) {
        printlns("Cleared all non-localhost entries");
        0
    } else {
        eprintlns("Error: Failed to write /etc/hosts");
        1
    }
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        print_usage();
        return 1;
    }

    let cmd = unsafe {
        let arg = *argv.add(1);
        let len = strlen(arg);
        core::str::from_utf8(core::slice::from_raw_parts(arg, len)).unwrap_or("")
    };

    match cmd {
        "add" => {
            if argc < 4 {
                eprintlns("Error: add requires <hostname> <ip>");
                return 1;
            }

            let hostname = unsafe {
                let arg = *argv.add(2);
                let len = strlen(arg);
                core::str::from_utf8(core::slice::from_raw_parts(arg, len)).unwrap_or("")
            };

            let ip = unsafe {
                let arg = *argv.add(3);
                let len = strlen(arg);
                core::str::from_utf8(core::slice::from_raw_parts(arg, len)).unwrap_or("")
            };

            cmd_add(hostname, ip)
        }
        "remove" | "rm" => {
            if argc < 3 {
                eprintlns("Error: remove requires <hostname>");
                return 1;
            }

            let hostname = unsafe {
                let arg = *argv.add(2);
                let len = strlen(arg);
                core::str::from_utf8(core::slice::from_raw_parts(arg, len)).unwrap_or("")
            };

            cmd_remove(hostname)
        }
        "list" | "ls" => cmd_list(),
        "lookup" => {
            if argc < 3 {
                eprintlns("Error: lookup requires <hostname>");
                return 1;
            }

            let hostname = unsafe {
                let arg = *argv.add(2);
                let len = strlen(arg);
                core::str::from_utf8(core::slice::from_raw_parts(arg, len)).unwrap_or("")
            };

            cmd_lookup(hostname)
        }
        "clear" => cmd_clear(),
        _ => {
            prints("Error: Unknown command '");
            prints(cmd);
            printlns("'");
            print_usage();
            1
        }
    }
}
