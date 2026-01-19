//! which - locate a command

#![no_std]
#![no_main]

use efflux_libc::*;

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        eprintln("usage: which <command>...");
        return 1;
    }

    let mut status = 0;

    for i in 1..argc {
        let cmd = unsafe { cstr_to_str(*argv.add(i as usize)) };

        if !find_command(cmd) {
            status = 1;
        }
    }

    status
}

fn find_command(cmd: &str) -> bool {
    // Check if absolute path
    if cmd.starts_with('/') {
        let fd = open2(cmd, O_RDONLY);
        if fd >= 0 {
            close(fd);
            println(cmd);
            return true;
        }
        return false;
    }

    // Search in PATH directories (simplified: just check /bin)
    let dirs = ["/bin", "/sbin", "/usr/bin", "/usr/sbin"];

    for dir in &dirs {
        let mut path = [0u8; 256];
        let dir_len = dir.len();
        path[..dir_len].copy_from_slice(dir.as_bytes());
        path[dir_len] = b'/';
        let cmd_bytes = cmd.as_bytes();
        let cmd_len = cmd_bytes.len().min(256 - dir_len - 2);
        path[dir_len + 1..dir_len + 1 + cmd_len].copy_from_slice(&cmd_bytes[..cmd_len]);
        path[dir_len + 1 + cmd_len] = 0;

        let path_str = bytes_to_str(&path);
        let fd = open2(path_str, O_RDONLY);
        if fd >= 0 {
            close(fd);
            println(path_str);
            return true;
        }
    }

    false
}

fn bytes_to_str(bytes: &[u8]) -> &str {
    let mut len = 0;
    while len < bytes.len() && bytes[len] != 0 {
        len += 1;
    }
    unsafe { core::str::from_utf8_unchecked(&bytes[..len]) }
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
