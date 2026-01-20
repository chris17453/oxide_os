//! env - print environment variables
//!
//! With no arguments, prints all environment variables.

#![no_std]
#![no_main]

use libc::*;

/// Print bytes until null terminator
fn print_bytes(s: &[u8]) {
    for &b in s {
        if b == 0 {
            break;
        }
        putchar(b);
    }
}

#[unsafe(no_mangle)]
fn main() -> i32 {
    // Print all environment variables
    env_iter(|name, value| {
        print_bytes(name);
        prints("=");
        print_bytes(value);
        printlns("");
    });
    0
}
