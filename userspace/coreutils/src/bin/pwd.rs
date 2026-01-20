//! pwd - print name of current/working directory
//!
//! Print the full filename of the current working directory.

#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
fn main(_argc: i32, _argv: *const *const u8) -> i32 {
    let mut buf = [0u8; 1024];

    if getcwd(&mut buf) < 0 {
        eprintlns("pwd: error getting current directory");
        return 1;
    }

    // Print until null terminator
    for &c in buf.iter() {
        if c == 0 {
            break;
        }
        putchar(c);
    }
    printlns("");

    0
}
