//! pkill - kill processes by name

#![no_std]
#![no_main]
#![allow(unused)]

use libc::*;

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        eprintlns("usage: pkill [-signal] <pattern>");
        return 1;
    }

    let mut signal = 15i32; // Default: SIGTERM
    let mut pattern_idx = 1;

    // Parse optional signal
    let arg1 = unsafe { cstr_to_str(*argv.add(1)) };
    if arg1.starts_with('-') && arg1.len() > 1 {
        let sig_str = &arg1.as_bytes()[1..];
        signal = match parse_int(sig_str) {
            Some(v) => v as i32,
            None => {
                // Try named signals
                match sig_str {
                    b"TERM" => 15,
                    b"KILL" => 9,
                    b"HUP" => 1,
                    b"INT" => 2,
                    b"QUIT" => 3,
                    b"USR1" => 10,
                    b"USR2" => 12,
                    b"STOP" => 19,
                    b"CONT" => 18,
                    _ => {
                        eprintlns("pkill: invalid signal");
                        return 1;
                    }
                }
            }
        };
        pattern_idx = 2;
    }

    if pattern_idx >= argc {
        eprintlns("pkill: missing pattern");
        return 1;
    }

    let pattern = unsafe { cstr_to_str(*argv.add(pattern_idx as usize)) };

    // In a full implementation, we would:
    // 1. Read /proc directory
    // 2. For each process, read its cmdline
    // 3. Match against pattern
    // 4. Send signal to matching processes

    prints("pkill: would send signal ");
    print_i64(signal as i64);
    prints(" to processes matching '");
    prints(pattern);
    printlns("'");

    // Note: This requires /proc filesystem support
    eprintlns("pkill: /proc filesystem not yet fully implemented");

    1
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

trait StrExt {
    fn starts_with(&self, prefix: &str) -> bool;
}

impl StrExt for &str {
    fn starts_with(&self, prefix: &str) -> bool {
        if self.len() < prefix.len() {
            return false;
        }
        let self_bytes = self.as_bytes();
        let prefix_bytes = prefix.as_bytes();
        for i in 0..prefix.len() {
            if self_bytes[i] != prefix_bytes[i] {
                return false;
            }
        }
        true
    }
}
