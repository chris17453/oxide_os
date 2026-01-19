//! clear - clear the terminal screen

#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
fn main() -> i32 {
    // ANSI escape sequence to clear screen and move cursor to home
    print("\x1b[2J\x1b[H");
    0
}
