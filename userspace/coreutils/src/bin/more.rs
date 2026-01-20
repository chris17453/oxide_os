//! more - file perusal filter for viewing
//!
//! Display file contents one screen at a time.

#![no_std]
#![no_main]

use libc::*;

const LINES_PER_PAGE: usize = 24;

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
    // Read from stdin if no file specified
    if argc < 2 {
        return page_fd(STDIN_FILENO);
    }

    let mut status = 0;

    for i in 1..argc {
        let filename = ptr_to_str(unsafe { *argv.add(i as usize) });

        // Print filename header if multiple files
        if argc > 2 {
            if i > 1 {
                printlns("");
            }
            prints(":::::::::::::::\n");
            prints(filename);
            prints("\n:::::::::::::::\n");
        }

        let fd = open(filename, O_RDONLY, 0);
        if fd < 0 {
            prints("more: ");
            prints(filename);
            prints(": No such file or directory\n");
            status = 1;
            continue;
        }

        if page_fd(fd) != 0 {
            close(fd);
            return 0; // User quit
        }

        close(fd);
    }

    status
}

fn page_fd(fd: i32) -> i32 {
    let mut buf = [0u8; 4096];
    let mut line_count = 0;
    let mut line_buf = [0u8; 1024];
    let mut line_len = 0;

    loop {
        let n = read(fd, &mut buf);
        if n <= 0 {
            // Print any remaining line
            if line_len > 0 {
                for i in 0..line_len {
                    putchar(line_buf[i]);
                }
                printlns("");
            }
            break;
        }

        for i in 0..n as usize {
            let c = buf[i];

            if c == b'\n' {
                // Print the line
                for j in 0..line_len {
                    putchar(line_buf[j]);
                }
                printlns("");

                line_len = 0;
                line_count += 1;

                // Check if we need to pause
                if line_count >= LINES_PER_PAGE {
                    prints("--More--");

                    // Wait for keypress
                    let mut key = [0u8; 1];
                    let key_fd = open("/dev/console", O_RDONLY, 0);
                    if key_fd >= 0 {
                        let _ = read(key_fd, &mut key);
                        close(key_fd);
                    }

                    // Clear the --More-- prompt
                    prints("\r        \r");

                    match key[0] {
                        b'q' | b'Q' => return 1, // Quit
                        b' ' => line_count = 0,   // Next page
                        b'\n' | b'\r' => line_count = LINES_PER_PAGE - 1, // Next line
                        _ => line_count = 0,
                    }
                }
            } else {
                // Buffer the character
                if line_len < line_buf.len() {
                    line_buf[line_len] = c;
                    line_len += 1;
                }
            }
        }
    }

    0
}
