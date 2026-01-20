//! od - dump files in octal and other formats
//!
//! Write an unambiguous representation, octal bytes by default, of FILE.

#![no_std]
#![no_main]

use libc::*;

const OCT_CHARS: &[u8] = b"01234567";
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

#[derive(Clone, Copy, PartialEq)]
enum OutputFormat {
    Octal,      // -o (default)
    Hex,        // -x
    Decimal,    // -d
    Char,       // -c
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        eprintlns("usage: od [-o|-x|-d|-c] file");
        return 1;
    }

    let mut format = OutputFormat::Octal;
    let mut file_arg = 1usize;

    // Parse options
    for i in 1..argc as usize {
        let arg = ptr_to_str(unsafe { *argv.add(i) });
        if arg.starts_with("-") {
            for c in arg.bytes().skip(1) {
                match c {
                    b'o' => format = OutputFormat::Octal,
                    b'x' => format = OutputFormat::Hex,
                    b'd' => format = OutputFormat::Decimal,
                    b'c' => format = OutputFormat::Char,
                    _ => {}
                }
            }
            file_arg = i + 1;
        } else {
            break;
        }
    }

    if file_arg >= argc as usize {
        eprintlns("od: missing file operand");
        return 1;
    }

    let filename = ptr_to_str(unsafe { *argv.add(file_arg) });

    let fd = open(filename, O_RDONLY, 0);
    if fd < 0 {
        prints("od: ");
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

        // Print offset in octal (7 digits)
        print_octal_offset(offset);
        prints(" ");

        match format {
            OutputFormat::Octal => {
                for i in 0..count {
                    print_octal_byte(buf[i]);
                    prints(" ");
                }
            }
            OutputFormat::Hex => {
                for i in 0..count {
                    putchar(HEX_CHARS[(buf[i] >> 4) as usize]);
                    putchar(HEX_CHARS[(buf[i] & 0xF) as usize]);
                    prints(" ");
                }
            }
            OutputFormat::Decimal => {
                for i in 0..count {
                    print_decimal_byte(buf[i]);
                    prints(" ");
                }
            }
            OutputFormat::Char => {
                for i in 0..count {
                    print_char_repr(buf[i]);
                    prints(" ");
                }
            }
        }

        printlns("");
        offset += count as u64;
    }

    // Print final offset
    print_octal_offset(offset);
    printlns("");

    close(fd);
    0
}

/// Print offset as 7-digit octal
fn print_octal_offset(val: u64) {
    for i in (0..7).rev() {
        let digit = ((val >> (i * 3)) & 0x7) as usize;
        putchar(OCT_CHARS[digit]);
    }
}

/// Print byte as 3-digit octal
fn print_octal_byte(val: u8) {
    putchar(OCT_CHARS[(val >> 6) as usize]);
    putchar(OCT_CHARS[((val >> 3) & 0x7) as usize]);
    putchar(OCT_CHARS[(val & 0x7) as usize]);
}

/// Print byte as 3-digit decimal
fn print_decimal_byte(val: u8) {
    if val >= 100 {
        putchar(b'0' + val / 100);
    } else {
        putchar(b' ');
    }
    if val >= 10 {
        putchar(b'0' + (val / 10) % 10);
    } else {
        putchar(b' ');
    }
    putchar(b'0' + val % 10);
}

/// Print character representation
fn print_char_repr(c: u8) {
    match c {
        0 => prints("\\0"),
        7 => prints("\\a"),
        8 => prints("\\b"),
        9 => prints("\\t"),
        10 => prints("\\n"),
        11 => prints("\\v"),
        12 => prints("\\f"),
        13 => prints("\\r"),
        32..=126 => {
            putchar(b' ');
            putchar(c);
        }
        _ => {
            // Print as octal escape
            putchar(b'\\');
            putchar(OCT_CHARS[(c >> 6) as usize]);
            putchar(OCT_CHARS[((c >> 3) & 0x7) as usize]);
            putchar(OCT_CHARS[(c & 0x7) as usize]);
        }
    }
}
