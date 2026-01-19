//! kill - send a signal to a process

#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
fn main() -> i32 {
    // In a real implementation, we'd get args from the kernel
    eprintln("kill: argument passing not implemented");
    1
}
