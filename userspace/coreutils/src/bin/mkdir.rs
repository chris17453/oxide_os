//! mkdir - make directories

#![no_std]
#![no_main]

use efflux_libc::*;

#[unsafe(no_mangle)]
fn main() -> i32 {
    // In a real implementation, we'd get args from the kernel
    // For now, just print a placeholder
    eprintln("mkdir: argument passing not implemented");
    1
}
