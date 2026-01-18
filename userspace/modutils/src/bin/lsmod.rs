//! lsmod - List loaded kernel modules
//!
//! Usage: lsmod

#![no_std]
#![no_main]

use efflux_libc::*;

/// Main entry point
#[unsafe(no_mangle)]
pub fn main() -> i32 {
    // Read /proc/modules to list loaded modules
    let fd = open2("/proc/modules", O_RDONLY);
    if fd < 0 {
        // /proc/modules not available, show header only
        println("Module                  Size  Used by");
        return 0;
    }

    // Print header
    println("Module                  Size  Used by");

    // Read and print module list
    let mut buf = [0u8; 4096];
    loop {
        let n = read(fd, &mut buf);
        if n <= 0 {
            break;
        }

        // Print the content
        for i in 0..n as usize {
            putchar(buf[i]);
        }
    }

    close(fd);
    0
}
