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

/// — GraveShift: Check if autologin is enabled via /etc/autologin.
/// Returns true if file exists and contains a valid username. — GraveShift
fn check_autologin(user_buf: &mut [u8; MAX_INPUT]) -> Option<&'static PasswdEntry> {
    let fd = open2("/etc/autologin", O_RDONLY);
    if fd < 0 {
        return None;
    }
    let mut buf = [0u8; 64];
    let n = read(fd, &mut buf);
    close(fd);
    if n <= 0 {
        return None;
    }
    // Trim trailing newline/whitespace
    let mut len = n as usize;
    while len > 0 && (buf[len - 1] == b'\n' || buf[len - 1] == b'\r' || buf[len - 1] == b' ') {
        len -= 1;
    }
    if len == 0 {
        return None;
    }
    user_buf[..len].copy_from_slice(&buf[..len]);
    user_buf[len] = 0;
    lookup_user(user_buf)
}

/// Main entry point
#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let mut attempts = 0;
    const MAX_ATTEMPTS: i32 = 3;

    // — GraveShift: autologin shortcut — if /etc/autologin exists with a username,
    // skip the prompt dance entirely. Because debugging Ctrl+C at 3 AM without
    // keyboard input is a special kind of hell. — GraveShift
    {
        let mut auto_user = [0u8; MAX_INPUT];
        if let Some(entry) = check_autologin(&mut auto_user) {
            prints("Auto-login: ");
            prints(entry.username);
            prints("\n");

            // Skip straight to shell spawn
            let pid = fork();
            if pid < 0 {
                prints("Failed to fork\n");
                return 1;
            }
            if pid == 0 {
                setenv("HOME", entry.home);
                setenv("USER", entry.username);
                setenv("SHELL", entry.shell);
                setenv("PWD", entry.home);
                let _ = chdir(entry.home);
                let _ = setgid(entry.gid);
                let _ = setuid(entry.uid);
                exec(entry.shell);
                prints("Failed to exec shell\n");
                exit(1);
            } else {
                let mut status = 0;
                waitpid(pid, &mut status, 0);
                prints("\n[login] Session ended\n");
                return 0;
            }
        }
    }

    loop {
        // Print login prompt
        prints("\nOXIDE OS login: ");
        fflush_stdout(); // — SoftGlyph: Flush prompt before blocking on read

        // — SoftGlyph: Don't manually echo — TTY line discipline handles it in canonical mode.
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
        fflush_stdout(); // — SoftGlyph: Flush password prompt before blocking on read
        let mut password = [0u8; MAX_INPUT];
        let pw_len = read_line(&mut password, false);
        prints("\n");

        if pw_len == 0 && load_password(entry.username, entry.default_password).len == 0 {
            // Empty password accepted
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

        // — GraveShift: Debug — trace fork return value. If parent gets 0 instead of
        // child_pid, it takes the child path and execs instead of waiting. This kills
        // the login session because nobody waits for the shell.
        {
            let mut buf = [0u8; 32];
            let mut i = 0;
            for &b in b"[LOGIN] fork=" { buf[i] = b; i += 1; }
            if pid < 0 {
                buf[i] = b'-'; i += 1;
                let abs = (-pid) as u32;
                if abs >= 10 { buf[i] = b'0' + ((abs / 10) % 10) as u8; i += 1; }
                buf[i] = b'0' + (abs % 10) as u8; i += 1;
            } else {
                let p = pid as u32;
                if p >= 10 { buf[i] = b'0' + ((p / 10) % 10) as u8; i += 1; }
                buf[i] = b'0' + (p % 10) as u8; i += 1;
            }
            buf[i] = b'\n'; i += 1;
            let _ = write(2, &buf[..i]);
        }

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
