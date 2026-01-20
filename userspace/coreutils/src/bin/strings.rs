//! strings - Print the printable strings in a file
//!
//! Find and print printable character sequences in files.

#![no_std]
#![no_main]

use libc::*;

const DEFAULT_MIN_LEN: usize = 4;

/// Convert a C string pointer to a Rust str slice
fn ptr_to_str(ptr: *const u8) -> &'static str {
    unsafe {
        let mut len = 0;
        while *ptr.add(len) != 0 {
            len += 1;
        }
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr, len))
    }
}

/// Parse an integer from a string
fn parse_usize(s: &str) -> Option<usize> {
    let mut result: usize = 0;
    for c in s.bytes() {
        if c >= b'0' && c <= b'9' {
            result = result.checked_mul(10)?.checked_add((c - b'0') as usize)?;
        } else {
            return None;
        }
    }
    Some(result)
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        eprintlns("usage: strings [-n min-len] file...");
        return 1;
    }

    let mut min_len = DEFAULT_MIN_LEN;
    let mut file_start = 1i32;

    // Parse -n option
    if argc > 2 {
        let arg1 = ptr_to_str(unsafe { *argv.add(1) });
        if arg1 == "-n" && argc > 3 {
            let len_str = ptr_to_str(unsafe { *argv.add(2) });
            min_len = parse_usize(len_str).unwrap_or(DEFAULT_MIN_LEN);
            file_start = 3;
        }
    }

    let mut status = 0;

    for i in file_start..argc {
        let filename = ptr_to_str(unsafe { *argv.add(i as usize) });
        if process_file(filename, min_len) != 0 {
            status = 1;
        }
    }

    status
}

fn process_file(filename: &str, min_len: usize) -> i32 {
    let fd = open(filename, O_RDONLY, 0);
    if fd < 0 {
        prints("strings: ");
        prints(filename);
        eprintlns(": No such file or directory");
        return 1;
    }

    let mut buf = [0u8; 4096];
    let mut string_buf = [0u8; 1024];
    let mut string_len = 0usize;

    loop {
        let n = read(fd, &mut buf);
        if n <= 0 {
            break;
        }

        for i in 0..n as usize {
            let c = buf[i];

            // Check if printable ASCII
            if c >= 0x20 && c <= 0x7e {
                if string_len < string_buf.len() {
                    string_buf[string_len] = c;
                    string_len += 1;
                }
            } else {
                // End of string - print if long enough
                if string_len >= min_len {
                    for j in 0..string_len {
                        putchar(string_buf[j]);
                    }
                    printlns("");
                }
                string_len = 0;
            }
        }
    }

    // Don't forget trailing string
    if string_len >= min_len {
        for j in 0..string_len {
            putchar(string_buf[j]);
        }
        printlns("");
    }

    close(fd);
    0
}
