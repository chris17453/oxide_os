//! clear - clear the terminal screen

#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
fn main() -> i32 {
    // ANSI escape sequence to clear screen and move cursor to home
    // — GlassSignal: Gotta flush stdout; buffered writes don't hit the terminal til newline/256 bytes
    prints("\x1b[2J\x1b[H");
    fflush_stdout();
    0
}
