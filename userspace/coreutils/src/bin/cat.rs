//! cat - concatenate files and print to stdout
//!
//! Enhanced implementation with:
//! - Multiple file arguments
//! - Line numbering (-n, -b)
//! - Show line endings (-E)
//! - Show tabs (-T)
//! - Show non-printing characters (-v)
//! - Squeeze blank lines (-s)
//! - Combined options (-A = -vET)
//! - Stdin support (when no files or '-' specified)

#![no_std]
#![no_main]

use libc::*;

struct CatConfig {
    number_all: bool,
    number_nonblank: bool,
    show_ends: bool,
    show_tabs: bool,
    show_nonprinting: bool,
    squeeze_blank: bool,
}

impl CatConfig {
    fn new() -> Self {
        CatConfig {
            number_all: false,
            number_nonblank: false,
            show_ends: false,
            show_tabs: false,
            show_nonprinting: false,
            squeeze_blank: false,
        }
    }
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

/// Print a byte with optional transformations
fn print_byte(byte: u8, config: &CatConfig) {
    if config.show_tabs && byte == b'\t' {
        putchar(b'^');
        putchar(b'I');
    } else if config.show_nonprinting && byte < 32 && byte != b'\n' && byte != b'\t' {
        // Show control characters as ^X
        putchar(b'^');
        putchar(byte + 64);
    } else if config.show_nonprinting && byte == 127 {
        // DEL character
        putchar(b'^');
        putchar(b'?');
    } else if config.show_nonprinting && byte >= 128 {
        // High bit characters - show as M-X
        prints("M-");
        if byte >= 128 + 32 && byte < 128 + 127 {
            putchar(byte - 128);
        } else if byte == 255 {
            putchar(b'^');
            putchar(b'?');
        } else {
            putchar(b'^');
            putchar(byte - 128 + 64);
        }
    } else {
        putchar(byte);
    }
}

/// Print a number with padding
fn print_number(num: u64) {
    let mut buf = [0u8; 20];
    let mut len = 0;
    let mut n = num;

    if n == 0 {
        buf[0] = b'0';
        len = 1;
    } else {
        while n > 0 {
            buf[len] = b'0' + (n % 10) as u8;
            n /= 10;
            len += 1;
        }
    }

    // Pad to 6 characters
    for _ in len..6 {
        putchar(b' ');
    }

    // Print in reverse (correct order)
    for i in (0..len).rev() {
        putchar(buf[i]);
    }

    prints("  ");
}

/// Process a file descriptor
fn cat_fd(fd: i32, config: &CatConfig) -> i32 {
    let mut buf = [0u8; 4096];
    let mut line_number = 1u64;
    let mut at_line_start = true;
    let mut last_was_blank = false;
    let mut current_line_blank = true;

    loop {
        let n = read(fd, &mut buf);
        if n <= 0 {
            break;
        }

        for i in 0..n as usize {
            let byte = buf[i];

            // Handle line numbering at start of line
            if at_line_start {
                let should_number = if config.number_nonblank {
                    !current_line_blank
                } else {
                    config.number_all
                };

                if should_number {
                    print_number(line_number);
                    line_number += 1;
                }

                at_line_start = false;
            }

            // Process the byte
            if byte == b'\n' {
                // End of line
                if config.show_ends {
                    putchar(b'$');
                }
                putchar(b'\n');

                // Squeeze blank lines
                if config.squeeze_blank {
                    if current_line_blank {
                        if last_was_blank {
                            // Skip this blank line
                            at_line_start = true;
                            current_line_blank = true;
                            continue;
                        }
                        last_was_blank = true;
                    } else {
                        last_was_blank = false;
                    }
                }

                at_line_start = true;
                current_line_blank = true;
            } else {
                // Regular character
                print_byte(byte, config);
                if byte != b' ' && byte != b'\t' {
                    current_line_blank = false;
                }
            }
        }
    }

    0
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    let mut config = CatConfig::new();
    let mut arg_idx = 1;

    // Parse options
    while arg_idx < argc {
        let arg_ptr = unsafe { *argv.add(arg_idx as usize) };
        let arg = cstr_to_str(arg_ptr);

        if arg.starts_with('-') && arg.len() > 1 && arg != "-" {
            for c in arg[1..].bytes() {
                match c {
                    b'n' => {
                        config.number_all = true;
                        config.number_nonblank = false;
                    }
                    b'b' => {
                        config.number_nonblank = true;
                        config.number_all = false;
                    }
                    b'E' => config.show_ends = true,
                    b'T' => config.show_tabs = true,
                    b'v' => config.show_nonprinting = true,
                    b's' => config.squeeze_blank = true,
                    b'A' => {
                        // Show all = -vET
                        config.show_nonprinting = true;
                        config.show_ends = true;
                        config.show_tabs = true;
                    }
                    b'e' => {
                        // Equivalent to -vE
                        config.show_nonprinting = true;
                        config.show_ends = true;
                    }
                    b't' => {
                        // Equivalent to -vT
                        config.show_nonprinting = true;
                        config.show_tabs = true;
                    }
                    _ => {
                        eprints("cat: unknown option: -");
                        putchar(c);
                        printlns("");
                        return 1;
                    }
                }
            }
            arg_idx += 1;
        } else {
            break;
        }
    }

    // If no files specified, read from stdin
    if arg_idx >= argc {
        return cat_fd(STDIN_FILENO, &config);
    }

    let mut status = 0;

    // Process each file
    for i in arg_idx..argc {
        let path = cstr_to_str(unsafe { *argv.add(i as usize) });

        if path == "-" {
            // Explicit stdin
            if cat_fd(STDIN_FILENO, &config) != 0 {
                status = 1;
            }
        } else {
            // Open file
            let fd = open2(path, O_RDONLY);
            if fd < 0 {
                eprints("cat: ");
                prints(path);
                eprintlns(": No such file or directory");
                status = 1;
                continue;
            }

            if cat_fd(fd, &config) != 0 {
                status = 1;
            }

            close(fd);
        }
    }

    status
}
