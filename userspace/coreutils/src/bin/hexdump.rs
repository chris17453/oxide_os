//! hexdump - Display file contents in hexadecimal
//!
//! ASCII, decimal, hexadecimal, octal dump.

#![no_std]
#![no_main]

use libc::*;

const HEX_CHARS: &[u8] = b"0123456789abcdef";

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

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        eprintlns("usage: hexdump [-C] file");
        return 1;
    }

    // Check for -C flag (canonical hex+ASCII display)
    let mut canonical = false;
    let mut file_arg = 1usize;

    if argc > 2 {
        let arg1 = ptr_to_str(unsafe { *argv.add(1) });
        if arg1 == "-C" {
            canonical = true;
            file_arg = 2;
        }
    }

    let filename = ptr_to_str(unsafe { *argv.add(file_arg) });

    let fd = open(filename, O_RDONLY, 0);
    if fd < 0 {
        prints("hexdump: ");
        prints(filename);
        eprintlns(": No such file or directory");
        return 1;
    }

    let mut buf = [0u8; 16];
    let mut offset: u64 = 0;

    loop {
        let n = read(fd, &mut buf);
        if n <= 0 {
            break;
        }

        let count = n as usize;

        if canonical {
            // Canonical format: offset | hex bytes | ASCII
            print_hex_u32(offset as u32);
            prints("  ");

            // Hex bytes
            for i in 0..16 {
                if i < count {
                    putchar(HEX_CHARS[(buf[i] >> 4) as usize]);
                    putchar(HEX_CHARS[(buf[i] & 0xF) as usize]);
                } else {
                    prints("  ");
                }
                if i == 7 {
                    prints("  ");
                } else {
                    putchar(b' ');
                }
            }

            prints(" |");

            // ASCII representation
            for i in 0..count {
                if buf[i] >= 0x20 && buf[i] <= 0x7e {
                    putchar(buf[i]);
                } else {
                    putchar(b'.');
                }
            }

            prints("|");
            printlns("");
        } else {
            // Default format: offset + words
            print_hex_u32(offset as u32);

            for i in (0..count).step_by(2) {
                putchar(b' ');
                if i + 1 < count {
                    // Little-endian word
                    putchar(HEX_CHARS[(buf[i + 1] >> 4) as usize]);
                    putchar(HEX_CHARS[(buf[i + 1] & 0xF) as usize]);
                    putchar(HEX_CHARS[(buf[i] >> 4) as usize]);
                    putchar(HEX_CHARS[(buf[i] & 0xF) as usize]);
                } else {
                    prints("  ");
                    putchar(HEX_CHARS[(buf[i] >> 4) as usize]);
                    putchar(HEX_CHARS[(buf[i] & 0xF) as usize]);
                }
            }

            printlns("");
        }

        offset += count as u64;
    }

    // Print final offset
    if canonical {
        print_hex_u32(offset as u32);
        printlns("");
    }

    close(fd);
    0
}

/// Print a 32-bit value as 8 hex digits
fn print_hex_u32(val: u32) {
    for i in (0..8).rev() {
        let nibble = ((val >> (i * 4)) & 0xF) as usize;
        putchar(HEX_CHARS[nibble]);
    }
}
