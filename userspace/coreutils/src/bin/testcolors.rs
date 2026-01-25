//! Simple color test utility to exercise terminal SGR support.
//!
//! Prints a matrix of standard, bright, 256-color cube, and truecolor gradients.

#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
fn main() -> i32 {
    print_standard();
    print_bright();
    print_256_cube();
    print_truecolor();
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
}

fn prints(s: &str) {
    for b in s.as_bytes() {
        putchar(*b);
    }
}

fn printd(mut n: u8) {
    let mut buf = [0u8; 4];
    let mut i = buf.len();
    if n == 0 {
        buf[buf.len() - 1] = b'0';
        i = buf.len() - 1;
    } else {
        while n > 0 {
            i -= 1;
            buf[i] = b'0' + (n % 10);
            n /= 10;
        }
    }
    for &b in &buf[i..] {
        putchar(b);
    }
}
