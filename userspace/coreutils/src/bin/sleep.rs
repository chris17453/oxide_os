//! sleep - delay for a specified time

#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        eprintlns("usage: sleep <seconds>");
        return 1;
    }

    let arg = unsafe { cstr_to_str(*argv.add(1)) };

    // Parse seconds (supports decimal like 0.5)
    let mut seconds = 0u64;
    let mut nanos = 0u64;
    let mut in_fraction = false;
    let mut fraction_digits = 0;

    for c in arg.bytes() {
        if c == b'.' {
            in_fraction = true;
            continue;
        }
        if c < b'0' || c > b'9' {
            eprintlns("sleep: invalid time interval");
            return 1;
        }
        let digit = (c - b'0') as u64;
        if in_fraction {
            if fraction_digits < 9 {
                nanos = nanos * 10 + digit;
                fraction_digits += 1;
            }
        } else {
            seconds = seconds * 10 + digit;
        }
    }

    // Adjust nanos to full nanoseconds
    while fraction_digits < 9 {
        nanos *= 10;
        fraction_digits += 1;
    }

    sys_nanosleep(seconds, nanos);

    0
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
