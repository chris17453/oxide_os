//! Login program for EFFLUX OS
//!
//! Prompts for username and password, authenticates, and spawns shell.

#![no_std]
#![no_main]

use efflux_libc::*;

/// Simple password entry (in real system would be in /etc/passwd)
struct PasswdEntry {
    username: &'static str,
    password_hash: &'static str,
    uid: u32,
    gid: u32,
    home: &'static str,
    shell: &'static str,
}

/// Built-in user database (in real system, read from /etc/passwd and /etc/shadow)
static USERS: &[PasswdEntry] = &[
    PasswdEntry {
        username: "root",
        password_hash: "", // Empty password for root in testing
        uid: 0,
        gid: 0,
        home: "/root",
        shell: "/bin/esh",
    },
    PasswdEntry {
        username: "user",
        password_hash: "", // Empty password
        uid: 1000,
        gid: 1000,
        home: "/home/user",
        shell: "/bin/esh",
    },
];

/// Maximum input length
const MAX_INPUT: usize = 256;

/// Read a line from stdin (without echo for password)
fn read_line(buf: &mut [u8], echo: bool) -> usize {
    let mut i = 0;
    loop {
        let mut c = [0u8; 1];
        if read(0, &mut c) <= 0 {
            break;
        }

        if c[0] == b'\n' || c[0] == b'\r' {
            if echo {
                print("\n");
            }
            break;
        }

        if c[0] == 127 || c[0] == 8 {
            // Backspace
            if i > 0 {
                i -= 1;
                if echo {
                    print("\x08 \x08");
                }
            }
            continue;
        }

        if i < buf.len() - 1 {
            buf[i] = c[0];
            i += 1;
            if echo {
                let s = [c[0]];
                if let Ok(ch) = core::str::from_utf8(&s) {
                    print(ch);
                }
            }
        }
    }
    buf[i] = 0;
    i
}

/// Simple string comparison
fn str_eq(a: &[u8], b: &str) -> bool {
    let b_bytes = b.as_bytes();
    let a_len = a.iter().position(|&c| c == 0).unwrap_or(a.len());

    if a_len != b_bytes.len() {
        return false;
    }

    for i in 0..a_len {
        if a[i] != b_bytes[i] {
            return false;
        }
    }
    true
}

/// Verify password (trivial implementation - just checks empty or matching)
fn verify_password(entry: &PasswdEntry, password: &[u8]) -> bool {
    // Empty password hash means no password required
    if entry.password_hash.is_empty() {
        return true;
    }

    // In a real system, this would hash the password and compare
    str_eq(password, entry.password_hash)
}

/// Look up user in database
fn lookup_user(username: &[u8]) -> Option<&'static PasswdEntry> {
    for entry in USERS {
        if str_eq(username, entry.username) {
            return Some(entry);
        }
    }
    None
}

/// Main entry point
#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let mut attempts = 0;
    const MAX_ATTEMPTS: i32 = 3;

    loop {
        // Print login prompt
        print("\nEFFLUX OS login: ");

        // Read username
        let mut username = [0u8; MAX_INPUT];
        let ulen = read_line(&mut username, true);
        if ulen == 0 {
            continue;
        }

        // Look up user
        let entry = match lookup_user(&username) {
            Some(e) => e,
            None => {
                print("Login incorrect\n");
                attempts += 1;
                if attempts >= MAX_ATTEMPTS {
                    print("Too many failed attempts\n");
                    return 1;
                }
                continue;
            }
        };

        // Prompt for password (if user has one)
        if !entry.password_hash.is_empty() {
            print("Password: ");
            let mut password = [0u8; MAX_INPUT];
            read_line(&mut password, false);
            print("\n");

            if !verify_password(entry, &password) {
                print("Login incorrect\n");
                attempts += 1;
                if attempts >= MAX_ATTEMPTS {
                    print("Too many failed attempts\n");
                    return 1;
                }
                continue;
            }
        }

        // Successful login - print welcome and spawn shell
        print("Welcome to EFFLUX OS, ");
        print(entry.username);
        print("!\n\n");

        // Fork and exec shell
        let pid = fork();
        if pid < 0 {
            print("Failed to fork\n");
            return 1;
        }

        if pid == 0 {
            // Child - exec shell
            exec(entry.shell);
            print("Failed to exec shell\n");
            exit(1);
        } else {
            // Parent - wait for shell
            let mut status = 0;
            waitpid(pid, &mut status, 0);

            // Shell exited - loop back to login prompt
            print("\n");
        }

        attempts = 0; // Reset after successful login
    }
}
