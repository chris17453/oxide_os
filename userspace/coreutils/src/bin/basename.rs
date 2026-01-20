//! basename - strip directory from filename

#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        eprintlns("usage: basename <path> [suffix]");
        return 1;
    }

    let path = unsafe { cstr_to_str(*argv.add(1)) };
    let suffix = if argc > 2 {
        Some(unsafe { cstr_to_str(*argv.add(2)) })
    } else {
        None
    };

    // Find last /
    let bytes = path.as_bytes();
    let mut start = 0;
    for i in 0..bytes.len() {
        if bytes[i] == b'/' {
            start = i + 1;
        }
    }

    let base = &bytes[start..];

    // Remove suffix if specified
    let end = if let Some(suf) = suffix {
        let suf_bytes = suf.as_bytes();
        if base.len() > suf_bytes.len() && base.ends_with(suf_bytes) {
            base.len() - suf_bytes.len()
        } else {
            base.len()
        }
    } else {
        base.len()
    };

    for i in 0..end {
        putchar(base[i]);
    }
    printlns("");

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
