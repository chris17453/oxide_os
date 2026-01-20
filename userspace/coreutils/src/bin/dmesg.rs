//! dmesg - Print or control the kernel ring buffer
//!
//! Display messages from the kernel ring buffer.

#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
fn main() -> i32 {
    // Read from /proc/kmsg or /dev/kmsg
    // For now, try to read from /proc/dmesg (if we implement it)
    let fd = open("/proc/dmesg", O_RDONLY, 0);
    if fd < 0 {
        // Fallback: show a simple message
        printlns("[    0.000000] Oxide OS kernel log");
        printlns("[    0.000001] No kernel message buffer available");
        printlns("[    0.000002] Use serial console for boot messages");
        return 0;
    }

    // Read and print the kernel log
    let mut buf = [0u8; 4096];
    loop {
        let n = read(fd, &mut buf);
        if n <= 0 {
            break;
        }
        for i in 0..n as usize {
            putchar(buf[i]);
        }
    }

    close(fd);
    0
}
