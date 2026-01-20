//! chmod - Change file mode bits
//!
//! Usage: chmod MODE FILE...

#![no_std]
#![no_main]

use libc::*;

// Store argv globally for get_arg function
static mut ARGV: *const *const u8 = core::ptr::null();
static mut ARGC: usize = 0;

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    unsafe {
        ARGC = argc as usize;
        ARGV = argv;
    }

    let argc = argc as usize;
    if argc < 3 {
        eputs("usage: chmod MODE FILE...\n");
        return 1;
    }

    let mode_arg = get_arg(1);
    let mut exit_code = 0;

    for i in 2..argc {
        let file = get_arg(i);
        if chmod_file(mode_arg, file) != 0 {
            exit_code = 1;
        }
    }

    exit_code
}

/// Get argument at index as byte slice
fn get_arg(idx: usize) -> &'static [u8] {
    unsafe {
        if ARGV.is_null() || idx >= ARGC {
            return b"";
        }
        let ptr = *ARGV.add(idx);
        if ptr.is_null() {
            return b"";
        }
        let mut len = 0;
        while *ptr.add(len) != 0 {
            len += 1;
        }
        core::slice::from_raw_parts(ptr, len)
    }
}

/// Convert byte slice to str for libc calls
fn to_str(s: &[u8]) -> &str {
    unsafe { core::str::from_utf8_unchecked(s) }
}

fn chmod_file(mode_arg: &[u8], path: &[u8]) -> i32 {
    // Get current file mode
    let mut st = Stat::zeroed();
    if stat(to_str(path), &mut st) != 0 {
        eprint!("chmod: cannot access '");
        write_bytes(STDERR_FILENO, path);
        eputs("'\n");
        return 1;
    }

    // Parse mode
    let new_mode = match parse_mode(mode_arg, st.mode) {
        Some(m) => m,
        None => {
            eprint!("chmod: invalid mode '");
            write_bytes(STDERR_FILENO, mode_arg);
            eputs("'\n");
            return 1;
        }
    };

    // Apply new mode using syscall
    if sys_chmod(to_str(path), new_mode) != 0 {
        eprint!("chmod: cannot change permissions of '");
        write_bytes(STDERR_FILENO, path);
        eputs("'\n");
        return 1;
    }

    0
}

fn write_bytes(fd: i32, s: &[u8]) {
    syscall::sys_write(fd, s);
}

/// Parse mode string - octal or symbolic
fn parse_mode(mode_arg: &[u8], current_mode: u32) -> Option<u32> {
    // Try octal first
    if let Some(mode) = parse_octal(mode_arg) {
        return Some(mode);
    }

    // Try symbolic mode
    parse_symbolic(mode_arg, current_mode)
}

/// Parse octal mode (e.g., "755")
fn parse_octal(s: &[u8]) -> Option<u32> {
    if s.is_empty() {
        return None;
    }

    let mut result: u32 = 0;
    for &c in s {
        if c < b'0' || c > b'7' {
            return None;
        }
        result = result * 8 + (c - b'0') as u32;
    }

    Some(result)
}

/// Parse symbolic mode (e.g., "u+x", "go-w", "a=rw")
fn parse_symbolic(s: &[u8], current: u32) -> Option<u32> {
    let mut mode = current & 0o7777;
    let mut i = 0;

    while i < s.len() {
        // Parse who: [ugoa]
        let mut who_mask: u32 = 0;
        while i < s.len() {
            match s[i] {
                b'u' => who_mask |= 0o700,
                b'g' => who_mask |= 0o070,
                b'o' => who_mask |= 0o007,
                b'a' => who_mask |= 0o777,
                _ => break,
            }
            i += 1;
        }

        // Default to 'a' if no who specified
        if who_mask == 0 {
            who_mask = 0o777;
        }

        // Parse operator: [+-=]
        if i >= s.len() {
            return None;
        }

        let op = s[i];
        if op != b'+' && op != b'-' && op != b'=' {
            return None;
        }
        i += 1;

        // Parse permission: [rwxXst]
        let mut perm: u32 = 0;
        while i < s.len() && s[i] != b',' {
            match s[i] {
                b'r' => perm |= 0o444,
                b'w' => perm |= 0o222,
                b'x' => perm |= 0o111,
                b'X' => {
                    // Execute only if directory or already has execute
                    if (current & 0o040000) != 0 || (current & 0o111) != 0 {
                        perm |= 0o111;
                    }
                }
                b's' => perm |= 0o6000, // setuid/setgid
                b't' => perm |= 0o1000, // sticky bit
                _ => return None,
            }
            i += 1;
        }

        // Apply the permission mask
        let effective_perm = perm & who_mask;

        match op {
            b'+' => mode |= effective_perm,
            b'-' => mode &= !effective_perm,
            b'=' => {
                mode &= !who_mask;
                mode |= effective_perm;
            }
            _ => {}
        }

        // Skip comma if present
        if i < s.len() && s[i] == b',' {
            i += 1;
        }
    }

    Some(mode)
}

/// Syscall wrapper for chmod
fn sys_chmod(path: &str, mode: u32) -> i32 {
    const CHMOD: u64 = 150;
    syscall::syscall3(CHMOD, path.as_ptr() as usize, path.len(), mode as usize) as i32
}
