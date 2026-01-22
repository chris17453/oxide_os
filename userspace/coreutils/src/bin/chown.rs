//! chown - change file owner and group

#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 3 {
        eprintlns("usage: chown <owner>[:<group>] <file>");
        return 1;
    }

    let owner_spec = unsafe { cstr_to_str(*argv.add(1)) };
    let file = unsafe { cstr_to_str(*argv.add(2)) };

    // Parse owner:group
    let owner_bytes = owner_spec.as_bytes();
    let mut colon_pos = None;
    for i in 0..owner_bytes.len() {
        if owner_bytes[i] == b':' {
            colon_pos = Some(i);
            break;
        }
    }

    let (uid, gid) = if let Some(pos) = colon_pos {
        // Both owner and group specified
        let owner_str = &owner_bytes[..pos];
        let group_str = &owner_bytes[pos + 1..];

        let uid = match parse_int(owner_str) {
            Some(v) => v as u32,
            None => {
                eprintlns("chown: invalid owner");
                return 1;
            }
        };

        let gid = match parse_int(group_str) {
            Some(v) => v as u32,
            None => {
                eprintlns("chown: invalid group");
                return 1;
            }
        };

        (uid, gid)
    } else {
        // Only owner specified
        let uid = match parse_int(owner_bytes) {
            Some(v) => v as u32,
            None => {
                eprintlns("chown: invalid owner");
                return 1;
            }
        };

        // Keep existing group (-1 means don't change)
        (uid, 0xFFFFFFFF)
    };

    // Call chown syscall
    let file_ptr = unsafe { *argv.add(2) };
    let mut file_len = 0;
    while unsafe { *file_ptr.add(file_len) != 0 } {
        file_len += 1;
    }

    let result = chown(file_ptr, file_len, uid as i32, gid as i32);
    if result < 0 {
        eprints("chown: cannot change ownership of '");
        eprints(file);
        eprintlns("'");
        return 1;
    }

    0
}

fn parse_int(s: &[u8]) -> Option<i64> {
    if s.is_empty() {
        return None;
    }

    let mut result: i64 = 0;
    for &c in s {
        if c < b'0' || c > b'9' {
            return None;
        }
        result = result * 10 + (c - b'0') as i64;
    }

    Some(result)
}

fn cstr_to_str(ptr: *const u8) -> &'static str {
    if ptr.is_null() {
        return "";
    }
    let mut len = 0;
    unsafe {
        while *ptr.add(len) != 0 {
            len += 1;
        }
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr, len))
    }
}
