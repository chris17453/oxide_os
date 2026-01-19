//! hostname - show or set system hostname

#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
fn main() -> i32 {
    // Try to read from /etc/hostname
    let fd = open2("/etc/hostname", O_RDONLY);
    if fd >= 0 {
        let mut buf = [0u8; 256];
        let n = read(fd, &mut buf);
        close(fd);

        if n > 0 {
            // Print without trailing newline
            for i in 0..n as usize {
                if buf[i] == b'\n' || buf[i] == 0 {
                    break;
                }
                putchar(buf[i]);
            }
            println("");
            return 0;
        }
    }

    // Fallback
    println("localhost");
    0
}
