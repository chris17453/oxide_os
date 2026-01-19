//! ls - list directory contents

#![no_std]
#![no_main]

use efflux_libc::*;

/// Directory entry header (matches kernel's UserDirEntry)
#[repr(C)]
struct DirEntry {
    d_ino: u64,
    d_off: u64,
    d_reclen: u16,
    d_type: u8,
}

// File type constants
const DT_DIR: u8 = 4;
const DT_REG: u8 = 8;
const DT_LNK: u8 = 10;
const DT_CHR: u8 = 2;
const DT_BLK: u8 = 6;

/// Print a null-terminated string from a byte slice
fn print_name(name: &[u8]) {
    for &b in name {
        if b == 0 {
            break;
        }
        putchar(b);
    }
}

/// Get file type character for display
fn type_char(d_type: u8) -> u8 {
    match d_type {
        DT_DIR => b'd',
        DT_REG => b'-',
        DT_LNK => b'l',
        DT_CHR => b'c',
        DT_BLK => b'b',
        _ => b'?',
    }
}

/// Parse command line arguments
struct Args {
    long_format: bool,
    show_all: bool,
    path: Option<[u8; 256]>,
}

fn parse_args(argc: i32, argv: *const *const u8) -> Args {
    let mut args = Args {
        long_format: false,
        show_all: false,
        path: None,
    };

    for i in 1..argc {
        let arg = unsafe { *argv.add(i as usize) };
        if arg.is_null() {
            continue;
        }

        let first = unsafe { *arg };
        if first == b'-' {
            // Parse flags
            let mut j = 1;
            loop {
                let c = unsafe { *arg.add(j) };
                if c == 0 {
                    break;
                }
                match c {
                    b'l' => args.long_format = true,
                    b'a' => args.show_all = true,
                    _ => {}
                }
                j += 1;
            }
        } else {
            // Path argument
            let mut path = [0u8; 256];
            let mut j = 0;
            while j < 255 {
                let c = unsafe { *arg.add(j) };
                if c == 0 {
                    break;
                }
                path[j] = c;
                j += 1;
            }
            args.path = Some(path);
        }
    }

    args
}

fn list_directory(path: &[u8], args: &Args) -> i32 {
    // Build null-terminated path string
    let path_str = unsafe {
        let mut len = 0;
        while len < path.len() && path[len] != 0 {
            len += 1;
        }
        core::str::from_utf8_unchecked(&path[..len])
    };

    let fd = open(path_str, O_RDONLY | O_DIRECTORY, 0);
    if fd < 0 {
        eprint("ls: cannot access '");
        print_name(path);
        eprintln("': No such file or directory");
        return 1;
    }

    // Read directory entries
    let mut buf = [0u8; 4096];
    loop {
        let n = sys_getdents(fd, &mut buf);
        if n <= 0 {
            break;
        }

        // Parse directory entries
        let mut offset = 0;
        while offset < n as usize {
            // Read entry header
            let entry_ptr = buf.as_ptr().wrapping_add(offset) as *const DirEntry;
            let entry = unsafe { &*entry_ptr };

            // Get name (starts after header)
            let name_offset = offset + core::mem::size_of::<DirEntry>();
            let name = &buf[name_offset..];

            // Skip hidden files unless -a
            let first_char = if name_offset < buf.len() { name[0] } else { 0 };
            if !args.show_all && first_char == b'.' {
                offset += entry.d_reclen as usize;
                continue;
            }

            if args.long_format {
                // Print type
                putchar(type_char(entry.d_type));
                print("  ");

                // Print inode
                print_u64(entry.d_ino);
                print("  ");
            }

            // Print name
            print_name(name);

            // Add trailing / for directories
            if entry.d_type == DT_DIR {
                putchar(b'/');
            }

            println("");

            // Move to next entry
            offset += entry.d_reclen as usize;
        }
    }

    close(fd);
    0
}

fn print_u64(mut n: u64) {
    if n == 0 {
        putchar(b'0');
        return;
    }

    let mut buf = [0u8; 20];
    let mut i = 0;
    while n > 0 {
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }

    while i > 0 {
        i -= 1;
        putchar(buf[i]);
    }
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    let args = parse_args(argc, argv);

    let path = match &args.path {
        Some(p) => p.as_slice(),
        None => b".\0".as_slice(),
    };

    list_directory(path, &args)
}
