//! tee - read from stdin and write to stdout and files

#![no_std]
#![no_main]

use efflux_libc::*;

const MAX_FILES: usize = 8;

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    let mut append = false;
    let mut file_start = 1;

    // Parse flags
    if argc > 1 {
        let arg1 = unsafe { cstr_to_str(*argv.add(1)) };
        if arg1 == "-a" {
            append = true;
            file_start = 2;
        }
    }

    // Open output files
    let mut fds = [-1i32; MAX_FILES];
    let mut num_fds = 0;

    for i in file_start..argc {
        if num_fds >= MAX_FILES {
            break;
        }
        let path = unsafe { cstr_to_str(*argv.add(i as usize)) };
        let flags = if append {
            O_WRONLY | O_CREAT | O_APPEND
        } else {
            O_WRONLY | O_CREAT | O_TRUNC
        };
        let fd = open(path, flags, 0o644);
        if fd < 0 {
            eprint("tee: cannot open '");
            print(path);
            eprintln("'");
        } else {
            fds[num_fds] = fd;
            num_fds += 1;
        }
    }

    // Read from stdin, write to stdout and all files
    let mut buf = [0u8; 4096];
    loop {
        let n = read(STDIN_FILENO, &mut buf);
        if n <= 0 {
            break;
        }

        // Write to stdout
        write(STDOUT_FILENO, &buf[..n as usize]);

        // Write to all files
        for i in 0..num_fds {
            write(fds[i], &buf[..n as usize]);
        }
    }

    // Close files
    for i in 0..num_fds {
        close(fds[i]);
    }

    0
}

fn cstr_to_str(ptr: *const u8) -> &'static str {
    if ptr.is_null() {
        return "";
    }
    let mut len = 0;
    unsafe {
        while *ptr.add(len) != 0 {
            len += 1;
        }
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr, len))
    }
}
