//! echo - display a line of text
//!
//! Enhanced implementation with:
//! - -n option (no trailing newline)
//! - -e option (enable escape sequences)
//! - -E option (disable escape sequences, default)
//! - Full escape sequence support:
//!   \\, \a, \b, \c, \e, \f, \n, \r, \t, \v
//!   \0NNN (octal), \xHH (hex)

#![no_std]
#![no_main]

use libc::*;

fn parse_octal(bytes: &[u8], start: usize) -> (u8, usize) {
    let mut val = 0u8;
    let mut len = 0;

    for i in start..bytes.len() {
        if i >= start + 3 {
            break;
        }
        let b = bytes[i];
        if b >= b'0' && b <= b'7' {
            val = val.wrapping_mul(8).wrapping_add(b - b'0');
            len += 1;
        } else {
            break;
        }
    }

    (val, len)
}

fn parse_hex(bytes: &[u8], start: usize) -> (u8, usize) {
    let mut val = 0u8;
    let mut len = 0;

    for i in start..bytes.len() {
        if i >= start + 2 {
            break;
        }
        let b = bytes[i];
        let digit = if b >= b'0' && b <= b'9' {
            b - b'0'
        } else if b >= b'a' && b <= b'f' {
            b - b'a' + 10
        } else if b >= b'A' && b <= b'F' {
            b - b'A' + 10
        } else {
            break;
        };

        val = val.wrapping_mul(16).wrapping_add(digit);
        len += 1;
    }

    (val, len)
}

fn print_with_escapes(s: &str) -> bool {
    let bytes = s.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            let next = bytes[i + 1];
            match next {
                b'\\' => {
                    putchar(b'\\');
                    i += 2;
                }
                b'a' => {
                    putchar(0x07); // BEL
                    i += 2;
                }
                b'b' => {
                    putchar(0x08); // Backspace
                    i += 2;
                }
                b'c' => {
                    // Stop printing, no newline
                    return false;
                }
                b'e' => {
                    putchar(0x1B); // ESC
                    i += 2;
                }
                b'f' => {
                    putchar(0x0C); // Form feed
                    i += 2;
                }
                b'n' => {
                    putchar(b'\n');
                    i += 2;
                }
                b'r' => {
                    putchar(b'\r');
                    i += 2;
                }
                b't' => {
                    putchar(b'\t');
                    i += 2;
                }
                b'v' => {
                    putchar(0x0B); // Vertical tab
                    i += 2;
                }
                b'0'..=b'7' => {
                    // Octal escape
                    let (val, len) = parse_octal(bytes, i + 1);
                    if len > 0 {
                        putchar(val);
                        i += 1 + len;
                    } else {
                        putchar(bytes[i]);
                        i += 1;
                    }
                }
                b'x' => {
                    // Hex escape
                    if i + 2 < bytes.len() {
                        let (val, len) = parse_hex(bytes, i + 2);
                        if len > 0 {
                            putchar(val);
                            i += 2 + len;
                        } else {
                            putchar(bytes[i]);
                            i += 1;
                        }
                    } else {
                        putchar(bytes[i]);
                        i += 1;
                    }
                }
                _ => {
                    // Unknown escape, print backslash and character
                    putchar(b'\\');
                    i += 1;
                }
            }
        } else {
            putchar(bytes[i]);
            i += 1;
        }
    }

    true
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    let mut newline = true;
    let mut interpret_escapes = false;
    let mut arg_idx = 1;

    // Parse options
    while arg_idx < argc {
        let arg = unsafe { *argv.add(arg_idx as usize) };
        if arg.is_null() {
            arg_idx += 1;
            continue;
        }

        let first = unsafe { *arg };
        if first != b'-' {
            break;
        }

        // Convert to string
        let mut len = 0;
        while unsafe { *arg.add(len) != 0 } {
            len += 1;
        }
        let arg_bytes = unsafe { core::slice::from_raw_parts(arg, len) };

        if arg_bytes == b"-n" {
            newline = false;
            arg_idx += 1;
        } else if arg_bytes == b"-e" {
            interpret_escapes = true;
            arg_idx += 1;
        } else if arg_bytes == b"-E" {
            interpret_escapes = false;
            arg_idx += 1;
        } else {
            // Not an option, start of arguments
            break;
        }
    }

    let mut first_arg = true;
    let mut should_print_newline = newline;

    // Print arguments
    for i in arg_idx..argc {
        if !first_arg {
            putchar(b' ');
        }
        first_arg = false;

        let arg_ptr = unsafe { *argv.add(i as usize) };

        // Convert to str
        let mut len = 0;
        while unsafe { *arg_ptr.add(len) != 0 } {
            len += 1;
        }
        let arg_str =
            unsafe { core::str::from_utf8_unchecked(core::slice::from_raw_parts(arg_ptr, len)) };

        if interpret_escapes {
            if !print_with_escapes(arg_str) {
                // \c encountered, stop printing
                should_print_newline = false;
                break;
            }
        } else {
            // Print literally
            for &b in arg_str.as_bytes() {
                putchar(b);
            }
        }
    }

    if should_print_newline {
        putchar(b'\n');
    }

    0
}
