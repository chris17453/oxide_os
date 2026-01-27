//! Test VT output

#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
fn main(_argc: i32, _argv: *const *const u8) -> i32 {
    // Open tty1
    let fd = open2("/dev/tty1", O_RDWR);
    if fd < 0 {
        eprintlns("Failed to open /dev/tty1");
        return 1;
    }

    // Write test message
    let msg = "Hello from VT1!\n";
    write(fd, msg.as_bytes());

    // Read a line
    prints("Type something: ");
    let mut buf = [0u8; 100];
    let n = read(fd, &mut buf);

    prints("You typed: ");
    write(1, &buf[..n as usize]);

    close(fd);
    0
}
