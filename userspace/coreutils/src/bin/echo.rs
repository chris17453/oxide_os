//! echo - display a line of text

#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
fn main() -> i32 {
    // In a real implementation, we'd get args from the kernel
    // For now, just print a placeholder
    println("echo: argument passing not implemented");
    0
}
