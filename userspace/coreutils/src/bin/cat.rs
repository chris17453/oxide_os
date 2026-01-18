//! cat - concatenate files and print to stdout

#![no_std]
#![no_main]

use efflux_libc::*;

#[unsafe(no_mangle)]
fn main() -> i32 {
    // Read from stdin and write to stdout
    let mut buf = [0u8; 1024];
    loop {
        let n = read(STDIN_FILENO, &mut buf);
        if n <= 0 {
            break;
        }
        write(STDOUT_FILENO, &buf[..n as usize]);
    }
    0
}
