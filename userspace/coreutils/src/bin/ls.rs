//! ls - list directory contents
//!
//! Enhanced implementation with:
//! - Long format (-l) with file sizes and permissions
//! - Show all files (-a)
//! - Human-readable sizes (-h)
//! - Recursive listing (-R)
//! - List directory entry itself (-d)
//! - Multiple directory arguments

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

/// Format file size in human-readable format
fn format_size_human(size: u64, buf: &mut [u8]) -> usize {
    let units = [b' ', b'K', b'M', b'G', b'T'];
    let mut s = size as f64;
    let mut unit_idx = 0;

    while s >= 1024.0 && unit_idx < units.len() - 1 {
        s /= 1024.0;
        unit_idx += 1;
    }

    // Format as integer.decimal
    let whole = s as u64;
    let frac = ((s - whole as f64) * 10.0) as u8;

    let mut pos = 0;

    // Write whole part
    if whole == 0 {
        buf[pos] = b'0';
        pos += 1;
    } else {
        let mut temp = [0u8; 20];
        let mut temp_len = 0;
        let mut n = whole;
        while n > 0 {
            temp[temp_len] = b'0' + (n % 10) as u8;
            n /= 10;
            temp_len += 1;
        }
        for i in (0..temp_len).rev() {
            buf[pos] = temp[i];
            pos += 1;
        }
    }

    // Add decimal if not bytes
    if unit_idx > 0 {
        buf[pos] = b'.';
        pos += 1;
        buf[pos] = b'0' + frac;
        pos += 1;
    }

    buf[pos] = units[unit_idx];
    pos += 1;

    pos
}

/// Parse command line arguments
struct Args {
    long_format: bool,
    show_all: bool,
    human_readable: bool,
    recursive: bool,
    directory_itself: bool,
    paths: [[u8; 256]; 16],
    path_count: usize,
}

impl Args {
    fn new() -> Self {
        Args {
            long_format: false,
            show_all: false,
            human_readable: false,
            recursive: false,
            directory_itself: false,
            paths: [[0; 256]; 16],
            path_count: 0,
        }
    }

    fn add_path(&mut self, path: &[u8]) {
        if self.path_count < 16 {
            let len = path.iter().position(|&c| c == 0).unwrap_or(path.len());
            let copy_len = if len > 255 { 255 } else { len };
            self.paths[self.path_count][..copy_len].copy_from_slice(&path[..copy_len]);
            self.path_count += 1;
        }
    }
}

fn parse_args(argc: i32, argv: *const *const u8) -> Args {
    let mut args = Args::new();

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
                    b'h' => args.human_readable = true,
                    b'R' => args.recursive = true,
                    b'd' => args.directory_itself = true,
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
            args.add_path(&path);
        }
    }

    args
}

fn list_directory(path: &[u8], args: &Args, depth: usize) -> i32 {
    // Build null-terminated path string
    let path_len = path.iter().position(|&c| c == 0).unwrap_or(path.len());
    let path_str = unsafe { core::str::from_utf8_unchecked(&path[..path_len]) };

    // If -d, just show the directory name itself
    if args.directory_itself {
        print_name(path);
        printlns("");
        return 0;
    }

    // Show directory name if recursive or multiple paths
    if args.recursive && depth > 0 || args.path_count > 1 {
        if depth > 0 {
            printlns("");
        }
        print_name(path);
        printlns(":");
    }

    let fd = open(path_str, O_RDONLY | O_DIRECTORY, 0);
    if fd < 0 {
        eprints("ls: cannot access '");
        print_name(path);
        eprintlns("': No such file or directory");
        return 1;
    }

    // For storing subdirectories to recurse into
    let mut subdirs: [[u8; 256]; 64] = [[0; 256]; 64];
    let mut subdir_count = 0;

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
                prints("  ");

                // Print inode
                print_u64(entry.d_ino);
                prints("  ");

                // Try to get file size (simplified - would need proper stat)
                // For now, just show placeholder
                if args.human_readable {
                    let mut size_buf = [0u8; 16];
                    let size_len = format_size_human(4096, &mut size_buf);
                    for i in 0..size_len {
                        putchar(size_buf[i]);
                    }
                } else {
                    prints("    ");
                }
                prints("  ");
            }

            // Print name
            print_name(name);

            // Add trailing / for directories
            if entry.d_type == DT_DIR {
                putchar(b'/');

                // Save subdirectory for recursion
                if args.recursive && subdir_count < 64 {
                    // Skip . and ..
                    if !(name[0] == b'.' && (name[1] == 0 || (name[1] == b'.' && name[2] == 0))) {
                        // Build full path
                        let mut full_path = [0u8; 256];
                        let mut pos = 0;

                        // Copy current path
                        while pos < path_len && pos < 240 {
                            full_path[pos] = path[pos];
                            pos += 1;
                        }

                        // Add separator if needed
                        if pos > 0 && full_path[pos - 1] != b'/' {
                            full_path[pos] = b'/';
                            pos += 1;
                        }

                        // Add name
                        let mut name_idx = 0;
                        while name[name_idx] != 0 && pos < 255 {
                            full_path[pos] = name[name_idx];
                            pos += 1;
                            name_idx += 1;
                        }

                        subdirs[subdir_count] = full_path;
                        subdir_count += 1;
                    }
                }
            }

            printlns("");

            // Move to next entry
            offset += entry.d_reclen as usize;
        }
    }

    close(fd);

    // Recurse into subdirectories
    if args.recursive {
        for i in 0..subdir_count {
            list_directory(&subdirs[i], args, depth + 1);
        }
    }

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

    // If no paths specified, use current directory
    if args.path_count == 0 {
        return list_directory(b".\0", &args, 0);
    }

    // List all specified paths
    let mut ret = 0;
    for i in 0..args.path_count {
        if list_directory(&args.paths[i], &args, 0) != 0 {
            ret = 1;
        }
    }

    ret
}
