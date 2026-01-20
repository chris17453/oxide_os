//! df - Report file system disk space usage
//!
//! Display amount of disk space available on mounted file systems.

#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
fn main() -> i32 {
    // Print header
    printlns("Filesystem     1K-blocks      Used Available Use% Mounted on");

    // For now, show hardcoded virtual filesystem info
    // In a real implementation, we'd read from /proc/mounts and statfs each

    // Root filesystem (tmpfs)
    prints("tmpfs           ");
    print_size_field(1024 * 1024); // 1GB total
    print_size_field(0);           // 0 used
    print_size_field(1024 * 1024); // 1GB available
    prints("  0% /");
    printlns("");

    // devfs
    prints("devfs           ");
    print_size_field(0);
    print_size_field(0);
    print_size_field(0);
    prints("  -  /dev");
    printlns("");

    // procfs
    prints("proc            ");
    print_size_field(0);
    print_size_field(0);
    print_size_field(0);
    prints("  -  /proc");
    printlns("");

    // initramfs
    prints("initramfs       ");
    print_size_field(4096); // ~4MB
    print_size_field(4096);
    print_size_field(0);
    prints("100% /initramfs");
    printlns("");

    0
}

/// Print a size field right-aligned in 10 characters
fn print_size_field(kb: u64) {
    let mut buf = [b' '; 10];
    let mut val = kb;
    let mut pos = 9;

    if val == 0 {
        buf[pos] = b'0';
    } else {
        while val > 0 && pos > 0 {
            buf[pos] = b'0' + (val % 10) as u8;
            val /= 10;
            pos = pos.saturating_sub(1);
        }
    }

    for &c in &buf {
        putchar(c);
    }
}
