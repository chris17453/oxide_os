//! nice - run a program with modified scheduling priority

#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        eprintlns("usage: nice [-n increment] <command> [args...]");
        return 1;
    }

    let mut niceness = 10i32; // Default increment
    let mut cmd_start = 1;

    // Parse -n option
    let arg1 = unsafe { cstr_to_str(*argv.add(1)) };
    if arg1 == "-n" || arg1.starts_with("-n") {
        if arg1 == "-n" {
            if argc < 3 {
                eprintlns("nice: -n requires an argument");
                return 1;
            }
            let arg2 = unsafe { cstr_to_str(*argv.add(2)) };
            niceness = match parse_int(arg2.as_bytes()) {
                Some(v) => v as i32,
                None => {
                    eprintlns("nice: invalid increment");
                    return 1;
                }
            };
            cmd_start = 3;
        } else {
            // -n10 form
            let num_str = &arg1.as_bytes()[2..];
            niceness = match parse_int(num_str) {
                Some(v) => v as i32,
                None => {
                    eprintlns("nice: invalid increment");
                    return 1;
                }
            };
            cmd_start = 2;
        }
    }

    if cmd_start >= argc {
        eprintlns("nice: missing command");
        return 1;
    }

    // In a full implementation, we would:
    // 1. Call setpriority() syscall to adjust niceness
    // 2. Execute the command with exec()

    prints("nice: would run with niceness ");
    print_i64(niceness as i64);
    prints(": ");

    for i in cmd_start..argc {
        let arg = unsafe { cstr_to_str(*argv.add(i as usize)) };
        prints(arg);
        if i < argc - 1 {
            prints(" ");
        }
    }
    printlns("");

    // Note: setpriority syscall not yet implemented
    eprintlns("nice: setpriority syscall not yet implemented");

    1
}

fn parse_int(s: &[u8]) -> Option<i64> {
    if s.is_empty() {
        return None;
    }

    let mut result: i64 = 0;
    let mut negative = false;
    let mut start = 0;

    if s[0] == b'-' {
        negative = true;
        start = 1;
    } else if s[0] == b'+' {
        start = 1;
    }

    for i in start..s.len() {
        let c = s[i];
        if c < b'0' || c > b'9' {
            return None;
        }
        result = result * 10 + (c - b'0') as i64;
    }

    if negative {
        result = -result;
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
