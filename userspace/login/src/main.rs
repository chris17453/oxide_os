//! Login program for OXIDE OS
//!
//! Prompts for username and password, authenticates, and spawns shell.

#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
pub static mut ROOT_PASSWORD: [u8; MAX_INPUT] = [0; MAX_INPUT];
#[unsafe(no_mangle)]
pub static mut ROOT_PASSWORD_LEN: usize = 0;

/// Simple password entry (in real system would be in /etc/passwd)
struct PasswdEntry {
    username: &'static str,
    default_password: &'static str,
    uid: u32,
    gid: u32,
    home: &'static str,
    shell: &'static str,
}

/// Built-in user database (in real system, read from /etc/passwd and /etc/shadow)
static USERS: &[PasswdEntry] = &[
    PasswdEntry {
        username: "root",
        default_password: "root",
        uid: 0,
        gid: 0,
        home: "/root",
        shell: "/bin/esh",
    },
    PasswdEntry {
        username: "user",
        default_password: "user",
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
                prints("\n");
            }
            break;
        }

        if c[0] == 127 || c[0] == 8 {
            // Backspace
            if i > 0 {
                i -= 1;
                if echo {
                    prints("\x08 \x08");
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
                    prints(ch);
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
    let expected = load_password(entry.username, entry.default_password);
    let pw_len = password_len(password);
    prints("[DEBUG] expected_len=");
    print_i64(expected.len as i64);
    if let Ok(s) = core::str::from_utf8(&expected.data[..expected.len]) {
        prints(" expected=\"");
        prints(s);
        prints("\"\n");
    } else {
        prints(" expected=<non-utf8>\n");
    }
    prints("[DEBUG] input_len=");
    print_i64(pw_len as i64);
    if let Ok(s) = core::str::from_utf8(&password[..pw_len]) {
        prints(" input=\"");
        prints(s);
        prints("\"\n");
    } else {
        prints(" input=<non-utf8>\n");
    }
    if pw_len != expected.len {
        return false;
    }
    for i in 0..pw_len {
        if password[i] != expected.data[i] {
            return false;
        }
    }
    true
}

struct PasswordBuf {
    data: [u8; MAX_INPUT],
    len: usize,
}

/// Load password from /etc/passwd (format: username:password\n).
/// Falls back to default if file missing or entry not found.
fn load_password(username: &str, default: &str) -> PasswordBuf {
    let mut buf = [0u8; MAX_INPUT];
    let mut len = 0usize;

    // Override for root from in-memory update
    if username == "root" {
        unsafe {
            if ROOT_PASSWORD_LEN > 0 && ROOT_PASSWORD_LEN < MAX_INPUT {
                len = ROOT_PASSWORD_LEN;
                buf[..len].copy_from_slice(&ROOT_PASSWORD[..len]);
                buf[len] = 0;
                prints("[DEBUG] using in-memory root password override\n");
                return PasswordBuf { data: buf, len };
            }
        }
    }

    // Try to read /etc/passwd
    let fd = open2("/etc/passwd", O_RDONLY);
    if fd >= 0 {
        let mut file = [0u8; 1024];
        let n = read(fd, &mut file);
        close(fd);
        if n > 0 {
            let total = n as usize;
            let uname_bytes = username.as_bytes();
            let mut idx = 0;
            while idx < total {
                let start = idx;
                while idx < total && file[idx] != b'\n' {
                    idx += 1;
                }
                let line_end = idx;
                if idx < total && file[idx] == b'\n' {
                    idx += 1;
                }
                // Find colon separator
                let mut colon = line_end;
                let mut j = start;
                while j < line_end {
                    if file[j] == b':' {
                        colon = j;
                        break;
                    }
                    j += 1;
                }
                if colon == line_end {
                    continue;
                }
                let name_len = colon - start;
                if name_len == uname_bytes.len() && file[start..colon] == *uname_bytes {
                    prints("[DEBUG] /etc/passwd entry found for user\n");
                    // Copy password portion up to next colon (passwd fields: user:password:uid:gid:...)
                    let pw_start = colon + 1;
                    let mut pw_end = line_end;
                    let mut p = pw_start;
                    while p < line_end {
                        if file[p] == b':' {
                            pw_end = p;
                            break;
                        }
                        p += 1;
                    }
                    let mut k = pw_start;
                    while k < pw_end && len < buf.len() - 1 {
                        buf[len] = file[k];
                        len += 1;
                        k += 1;
                    }
                    buf[len] = 0;
                    return PasswordBuf { data: buf, len };
                }
            }
        }
    }

    // Fallback to default
    prints("[DEBUG] using default password\n");
    let def_bytes = default.as_bytes();
    len = def_bytes.len().min(buf.len() - 1);
    buf[..len].copy_from_slice(&def_bytes[..len]);
    buf[len] = 0;
    PasswordBuf { data: buf, len }
}

fn password_len(buf: &[u8]) -> usize {
    buf.iter().position(|&c| c == 0).unwrap_or(buf.len())
}

#[unsafe(no_mangle)]
pub extern "C" fn login_set_root_password(ptr: *const u8, len: usize) {
    unsafe {
        let copy_len = len.min(MAX_INPUT - 1);
        for i in 0..copy_len {
            ROOT_PASSWORD[i] = *ptr.add(i);
        }
        ROOT_PASSWORD[copy_len] = 0;
        ROOT_PASSWORD_LEN = copy_len;
    }
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
        prints("\nOXIDE OS login: ");

        // Read username — don't echo here; the TTY line discipline
        // echoes each keystroke in canonical mode already.
        let mut username = [0u8; MAX_INPUT];
        let ulen = read_line(&mut username, false);
        if ulen == 0 {
            continue;
        }

        // Look up user
        let entry = match lookup_user(&username) {
            Some(e) => e,
            None => {
                prints("Login incorrect\n");
                attempts += 1;
                if attempts >= MAX_ATTEMPTS {
                    prints("Too many failed attempts\n");
                    return 1;
                }
                continue;
            }
        };

        // Prompt for password
        prints("Password: ");
        let mut password = [0u8; MAX_INPUT];
        let pw_len = read_line(&mut password, false);
        prints("\n");

        if pw_len == 0 && load_password(entry.username, entry.default_password).len == 0 {
            prints("[DEBUG] accepting empty password\n");
        } else if !verify_password(entry, &password) {
            prints("Login incorrect\n");
            attempts += 1;
            if attempts >= MAX_ATTEMPTS {
                prints("Too many failed attempts\n");
                return 1;
            }
            continue;
        }

        // Successful login - print welcome and spawn shell
        prints("Welcome to OXIDE OS, ");
        prints(entry.username);
        prints("!\n\n");

        // Fork and exec shell
        let pid = fork();
        if pid < 0 {
            prints("Failed to fork\n");
            return 1;
        }

        if pid == 0 {
            // Child - exec shell
            // Set environment for session
            setenv("HOME", entry.home);
            setenv("USER", entry.username);
            setenv("SHELL", entry.shell);
            setenv("PWD", entry.home);

            // Switch to user's home
            let _ = chdir(entry.home);

            // Drop privileges to user (best-effort)
            let _ = setgid(entry.gid);
            let _ = setuid(entry.uid);

            exec(entry.shell);
            prints("Failed to exec shell\n");
            exit(1);
        } else {
            // Parent - wait for shell
            let mut status = 0;
            waitpid(pid, &mut status, 0);

            // Shell exited - exit login so getty can respawn us
            prints("\n[login] Session ended\n");
            return 0;
        }
    }
}
