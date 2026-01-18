//! ls - list directory contents

#![no_std]
#![no_main]

use efflux_libc::*;

#[unsafe(no_mangle)]
fn main() -> i32 {
    // Open current directory
    let fd = open(".", O_RDONLY | O_DIRECTORY, 0);
    if fd < 0 {
        eprintln("ls: cannot open directory");
        return 1;
    }

    // Read directory entries
    let mut buf = [0u8; 1024];
    loop {
        let n = sys_getdents(fd, &mut buf);
        if n <= 0 {
            break;
        }

        // Parse directory entries
        let mut offset = 0;
        while offset < n as usize {
            // Simple parsing - assumes fixed-size entries
            // In reality, directory entry format varies
            let name_start = offset + 24; // Skip d_ino (8) + d_off (8) + d_reclen (2) + d_type (1) + padding
            let mut name_end = name_start;
            while name_end < buf.len() && buf[name_end] != 0 {
                name_end += 1;
            }

            // Print the entry name
            if name_end > name_start {
                for i in name_start..name_end {
                    putchar(buf[i]);
                }
                println("");
            }

            // Move to next entry (assuming 32-byte entries for simplicity)
            offset += 32;
        }
    }

    close(fd);
    0
}
