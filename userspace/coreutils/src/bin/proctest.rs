//! proctest - test utility to read proc filesystem entries
//!
//! Reads and displays various /proc files to test the implementation

#![no_std]
#![no_main]

use libc::*;

/// Read a file and print its contents
fn cat_file(path: &str) {
    let fd = open(path, O_RDONLY as u32, 0);
    if fd < 0 {
        eprints("Error: cannot open ");
        eprintlns(path);
        return;
    }

    prints("==> ");
    prints(path);
    printlns(" <==");

    let mut buf = [0u8; 1024];
    loop {
        let n = read(fd, &mut buf);
        if n <= 0 {
            break;
        }
        // Print the buffer as a string
        if let Ok(s) = core::str::from_utf8(&buf[0..(n as usize)]) {
            prints(s);
        }
    }

    close(fd);
    printlns("");
}

#[unsafe(no_mangle)]
fn main() -> i32 {
    printlns("Testing OXIDE procfs implementation...");
    printlns("");

    // Test all new proc files
    cat_file("/proc/cpuinfo");
    cat_file("/proc/uptime");
    cat_file("/proc/loadavg");
    cat_file("/proc/stat");
    cat_file("/proc/version");
    cat_file("/proc/devices");
    cat_file("/proc/filesystems");
    cat_file("/proc/meminfo");

    printlns("All proc files tested!");
    0
}
