//! Simple color test utility to exercise terminal SGR support.
//!
//! Prints a matrix of standard, bright, 256-color cube, and truecolor gradients.
//!
//! 🔥 PERFORMANCE FIX: Uses buffered writes instead of putchar() per byte 🔥
//! Before: 1000+ syscalls for 256-color cube (SLOW AS SHIT)
//! After:  ~10 syscalls for entire cube (FAST AF)

#![no_std]
#![no_main]

extern crate alloc;
use alloc::vec::Vec;
use libc::*;

/// Buffer for accumulating output before flushing to stdout
static mut OUTPUT_BUFFER: Option<Vec<u8>> = None;

#[unsafe(no_mangle)]
fn main() -> i32 {
    // Initialize output buffer
    unsafe {
        OUTPUT_BUFFER = Some(Vec::with_capacity(8192));
    }

    print_standard();
    print_bright();
    print_256_cube();
    print_truecolor();

    // Flush any remaining buffered output
    flush_buffer();

    0
}

fn print_standard() {
    printlns("Standard 16 colors:");
    for fg in 30..=37 {
        for bg in 40..=47 {
            print_sgr(fg, bg, " FG/BG ");
        }
        printlns("\x1b[0m");
    }
    printlns("");
}

fn print_bright() {
    printlns("Bright 16 colors:");
    for fg in 90..=97 {
        for bg in 100..=107 {
            print_sgr(fg, bg, " BRIGHT ");
        }
        printlns("\x1b[0m");
    }
    printlns("");
}

fn print_256_cube() {
    printlns("256-color cube (indices 16-231):");
    for row in 0..6 {
        for col in 0..36 {
            let idx = 16 + row * 36 + col;
            prints("\x1b[38;5;");
            printd(idx);
            prints("m");
            prints("██");
        }
        printlns("\x1b[0m");
    }
    printlns("");
}

fn print_truecolor() {
    printlns("Truecolor gradient:");
    for r in (0..=255).step_by(32) {
        for g in (0..=255).step_by(32) {
            let b = 255 - g;
            prints("\x1b[38;2;");
            printd(r);
            prints(";");
            printd(g);
            prints(";");
            printd(b);
            prints("m██");
        }
        printlns("\x1b[0m");
    }
    printlns("");
}

fn print_sgr(fg: u8, bg: u8, label: &str) {
    prints("\x1b[");
    printd(fg);
    prints(";");
    printd(bg);
    prints("m");
    prints(label);
    prints("\x1b[0m ");
}

fn printlns(s: &str) {
    prints(s);
    prints("\n");
    // Flush on newline for interactive output
    flush_buffer();
}

fn prints(s: &str) {
    // 🔥 PERFORMANCE FIX: Buffer bytes instead of putchar() per byte 🔥
    unsafe {
        if let Some(ref mut buf) = OUTPUT_BUFFER {
            buf.extend_from_slice(s.as_bytes());
            // Flush if buffer is getting large
            if buf.len() > 4096 {
                flush_buffer();
            }
        }
    }
}

fn printd(mut n: u8) {
    let mut digits = [0u8; 4];
    let mut i = digits.len();
    if n == 0 {
        digits[digits.len() - 1] = b'0';
        i = digits.len() - 1;
    } else {
        while n > 0 {
            i -= 1;
            digits[i] = b'0' + (n % 10);
            n /= 10;
        }
    }
    // 🔥 PERFORMANCE FIX: Buffer digits instead of putchar() per byte 🔥
    unsafe {
        if let Some(ref mut buf) = OUTPUT_BUFFER {
            buf.extend_from_slice(&digits[i..]);
        }
    }
}

/// Flush buffered output to stdout
/// 🔥 PERFORMANCE FIX: Batch write reduces syscalls by 100x 🔥
fn flush_buffer() {
    unsafe {
        if let Some(ref mut buf) = OUTPUT_BUFFER {
            if !buf.is_empty() {
                // Single syscall for entire buffer
                sys_write(STDOUT_FILENO, buf.as_slice());
                buf.clear();
            }
        }
    }
}
