//! insmod - Insert a kernel module
//!
//! Usage: insmod <module.ko> [params...]

#![no_std]
#![no_main]

use efflux_libc::*;

/// Syscall number for init_module
const SYS_INIT_MODULE: u64 = 50;

/// Maximum module size (16 MB)
const MAX_MODULE_SIZE: usize = 16 * 1024 * 1024;

/// Read entire file into buffer
fn read_file(path: &str, buf: &mut [u8]) -> i32 {
    let fd = open2(path, O_RDONLY);
    if fd < 0 {
        return fd;
    }

    let mut total = 0usize;
    loop {
        let n = read(fd, &mut buf[total..]);
        if n < 0 {
            close(fd);
            return n as i32;
        }
        if n == 0 {
            break;
        }
        total += n as usize;
        if total >= buf.len() {
            close(fd);
            return -1; // Too big
        }
    }

    close(fd);
    total as i32
}

/// Main entry point
#[unsafe(no_mangle)]
pub fn main() -> i32 {
    // For now, just print usage since we don't have argv
    // In full implementation, we'd parse arguments

    println("insmod: Insert a kernel module");
    println("");
    println("Usage: insmod <module.ko> [params...]");
    println("");
    println("Note: Full implementation requires argument passing from kernel.");
    println("");

    // Demo: If we had arguments, we would:
    // 1. Open the .ko file
    // 2. Read it into memory
    // 3. Call sys_init_module syscall

    // Example implementation (commented out until argv works):
    /*
    let path = argv[1];
    let mut buf = [0u8; MAX_MODULE_SIZE];
    let size = read_file(path, &mut buf);
    if size < 0 {
        eprint("insmod: cannot read ");
        eprintln(path);
        return 1;
    }

    // Build params string from remaining arguments
    let params = if argc > 2 {
        // Concatenate argv[2..] with spaces
        ...
    } else {
        ""
    };

    let ret = syscall3(SYS_INIT_MODULE, buf.as_ptr() as u64, size as u64, params.as_ptr() as u64);
    if ret < 0 {
        eprintln("insmod: failed to insert module");
        return 1;
    }
    */

    0
}
