//! ps - report a snapshot of current processes

#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
fn main() -> i32 {
    printlns("  PID TTY          TIME CMD");

    // Read /proc to get process info
    // For now, just show current process
    let pid = getpid();
    prints("    ");
    print_i64(pid as i64);
    printlns(" ?        00:00:00 ps");

    0
}
