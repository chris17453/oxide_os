//! Simple color test utility to exercise terminal SGR support.
//!
//! Prints a matrix of standard, bright, 256-color cube, and truecolor gradients.
//!
//! — NeonVale: Every color in the 256-cube gets its moment. Backgrounds this time
//! so the dark ones aren't invisible on a dark terminal. Judge every shade fairly.

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
    print_256_bg();
    print_truecolor();

    // Final marker so we know the program actually completed
    // — NeonVale: If you see DONE, the process finished. If not, something hung.
    printlns("\x1b[0mDONE");

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

// — NeonVale: Use BACKGROUND 256-color so every shade is visible against the default fg.
// Foreground 256-color on a dark terminal hides the dark cube entries. BG shows everything.
fn print_256_bg() {
    printlns("256-color cube BG (indices 16-231):");
    for row in 0..6 {
        for col in 0..36 {
            let idx: u8 = 16 + row * 36 + col;
            // — NeonVale: Reset first, then set background color.
            // Two separate CSIs: simpler, avoids multi-param 256-color edge cases.
            prints("\x1b[0m\x1b[48;5;");
            printd(idx);
            prints("m  ");
        }
        printlns("\x1b[0m");
    }
    printlns("");

    // — NeonVale: Also show grayscale ramp (232-255) — 24 shades from dark to bright
    printlns("Grayscale ramp BG (indices 232-255):");
    for i in 0..24u8 {
        let idx: u8 = 232 + i;
        prints("\x1b[0m\x1b[48;5;");
        printd(idx);
        prints("m  ");
    }
    printlns("\x1b[0m");
    printlns("");
}

fn print_truecolor() {
    // — NeonVale: Use explicit loop indices to avoid u8 step_by overflow UB in debug builds.
    printlns("Truecolor gradient BG:");
    for ri in 0..8u8 {
        let r = ri * 32;
        for gi in 0..8u8 {
            let g = gi * 32;
            let b: u8 = 224 - g;
            prints("\x1b[0m\x1b[48;2;");
            printd(r);
            prints(";");
            printd(g);
            prints(";");
            printd(b);
            prints("m  ");
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
    // — NeonVale: Buffer bytes for batch writes — fewer syscalls, same result
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
    unsafe {
        if let Some(ref mut buf) = OUTPUT_BUFFER {
            buf.extend_from_slice(&digits[i..]);
        }
    }
}

/// Flush buffered output to stdout
fn flush_buffer() {
    unsafe {
        if let Some(ref mut buf) = OUTPUT_BUFFER {
            if !buf.is_empty() {
                sys_write(STDOUT_FILENO, buf.as_slice());
                buf.clear();
            }
        }
    }
}
