//! du - Estimate file space usage
//!
//! Summarize disk usage of each FILE, recursively for directories.

#![no_std]
#![no_main]

use libc::*;

/// Directory entry header (matches kernel's UserDirEntry)
#[repr(C)]
struct DirEntry {
    d_ino: u64,
    d_off: u64,
    d_reclen: u16,
    d_type: u8,
}

const DT_DIR: u8 = 4;
const DT_REG: u8 = 8;

/// Convert a C string pointer to a Rust str slice
fn ptr_to_str(ptr: *const u8) -> &'static str {
    unsafe {
        let mut len = 0;
        while *ptr.add(len) != 0 {
            len += 1;
        }
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr, len))
    }
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    let mut summarize = false;
    let mut human_readable = false;
    let mut file_start = 1i32;

    // Parse options
    for i in 1..argc {
        let arg = ptr_to_str(unsafe { *argv.add(i as usize) });
        if arg.starts_with("-") {
            for c in arg.bytes().skip(1) {
                match c {
                    b's' => summarize = true,
                    b'h' => human_readable = true,
                    _ => {}
                }
            }
            file_start = i + 1;
        } else {
            break;
        }
    }

    // Default to current directory if no path specified
    if file_start >= argc {
        let total = du_path(".", summarize, human_readable, 0);
        print_size(total as u64, human_readable);
        prints("\t.\n");
        return 0;
    }

    let mut status = 0;

    for i in file_start..argc {
        let path = ptr_to_str(unsafe { *argv.add(i as usize) });
        let total = du_path(path, summarize, human_readable, 0);
        if total < 0 {
            status = 1;
        } else {
            print_size(total as u64, human_readable);
            prints("\t");
            prints(path);
            prints("\n");
        }
    }

    status
}

/// Calculate disk usage for a path
/// Returns size in 1K blocks, or -1 on error
fn du_path(path: &str, summarize: bool, human_readable: bool, depth: i32) -> i64 {
    // First try to stat the path
    let mut st = Stat::zeroed();
    if stat(path, &mut st) < 0 {
        prints("du: cannot access '");
        prints(path);
        prints("': No such file or directory\n");
        return -1;
    }

    // If it's a regular file, return its size in 1K blocks
    if (st.mode & S_IFMT) == S_IFREG {
        let blocks = (st.size as i64 + 1023) / 1024;
        return blocks;
    }

    // If it's not a directory, return 0
    if (st.mode & S_IFMT) != S_IFDIR {
        return 0;
    }

    // It's a directory - recurse
    let fd = open(path, O_RDONLY | O_DIRECTORY, 0);
    if fd < 0 {
        prints("du: cannot read directory '");
        prints(path);
        prints("'\n");
        return -1;
    }

    let mut total: i64 = 4; // Directory itself uses at least 4K typically
    let mut buf = [0u8; 2048];

    loop {
        let n = sys_getdents(fd, &mut buf);
        if n <= 0 {
            break;
        }

        let mut offset = 0;
        while offset < n as usize {
            let entry_ptr = buf.as_ptr().wrapping_add(offset) as *const DirEntry;
            let entry = unsafe { &*entry_ptr };

            // Get name
            let name_offset = offset + core::mem::size_of::<DirEntry>();
            let name_bytes = &buf[name_offset..];

            // Find name length
            let mut name_len = 0;
            while name_len < name_bytes.len() && name_bytes[name_len] != 0 {
                name_len += 1;
            }

            let name = unsafe { core::str::from_utf8_unchecked(&name_bytes[..name_len]) };

            // Skip . and ..
            if name == "." || name == ".." {
                offset += entry.d_reclen as usize;
                continue;
            }

            // Build full path
            let mut full_path = [0u8; 512];
            let path_bytes = path.as_bytes();
            let mut idx = 0;

            // Copy path
            for &b in path_bytes {
                if idx < full_path.len() - 1 {
                    full_path[idx] = b;
                    idx += 1;
                }
            }

            // Add separator if needed
            if idx > 0 && full_path[idx - 1] != b'/' && idx < full_path.len() - 1 {
                full_path[idx] = b'/';
                idx += 1;
            }

            // Copy name
            for &b in name.as_bytes() {
                if idx < full_path.len() - 1 {
                    full_path[idx] = b;
                    idx += 1;
                }
            }

            let child_path = unsafe { core::str::from_utf8_unchecked(&full_path[..idx]) };

            // Recurse
            let child_size = du_path(child_path, summarize, human_readable, depth + 1);
            if child_size >= 0 {
                total += child_size;

                // Print non-summary output for subdirectories
                if !summarize && entry.d_type == DT_DIR {
                    print_size(child_size as u64, human_readable);
                    prints("\t");
                    prints(child_path);
                    prints("\n");
                }
            }

            offset += entry.d_reclen as usize;
        }
    }

    close(fd);
    total
}

/// Print size with optional human-readable format
fn print_size(size: u64, human_readable: bool) {
    if human_readable {
        if size >= 1024 * 1024 {
            print_u64(size / (1024 * 1024));
            prints("G");
        } else if size >= 1024 {
            print_u64(size / 1024);
            prints("M");
        } else {
            print_u64(size);
            prints("K");
        }
    } else {
        print_u64(size);
    }
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
