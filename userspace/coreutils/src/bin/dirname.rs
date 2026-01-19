//! dirname - strip last component from filename

#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        eprintln("usage: dirname <path>");
        return 1;
    }

    let path = unsafe { cstr_to_str(*argv.add(1)) };
    let bytes = path.as_bytes();

    // Find last /
    let mut last_slash = None;
    for i in 0..bytes.len() {
        if bytes[i] == b'/' {
            last_slash = Some(i);
        }
    }

    match last_slash {
        Some(0) => println("/"),
        Some(pos) => {
            for i in 0..pos {
                putchar(bytes[i]);
            }
            println("");
        }
        None => println("."),
    }

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
