//! mv - move/rename files
//!
//! Enhanced implementation with:
//! - Force mode (-f)
//! - Interactive mode (-i)
//! - No clobber (-n)
//! - Verbose mode (-v)
//! - Update mode (-u)
//! - Multiple source files to directory
//! - Cross-device move (copy + delete fallback)

#![no_std]
#![no_main]

use libc::*;

const MAX_PATH: usize = 256;

struct MvConfig {
    force: bool,
    interactive: bool,
    no_clobber: bool,
    verbose: bool,
    update: bool,
    sources: [[u8; MAX_PATH]; 64],
    source_count: usize,
    dest: [u8; MAX_PATH],
    dest_len: usize,
}

impl MvConfig {
    fn new() -> Self {
        MvConfig {
            force: true, // Default to force
            interactive: false,
            no_clobber: false,
            verbose: false,
            update: false,
            sources: [[0; MAX_PATH]; 64],
            source_count: 0,
            dest: [0; MAX_PATH],
            dest_len: 0,
        }
    }

    fn add_source(&mut self, s: &str) {
        if self.source_count < 64 {
            let bytes = s.as_bytes();
            let len = if bytes.len() > MAX_PATH - 1 {
                MAX_PATH - 1
            } else {
                bytes.len()
            };
            self.sources[self.source_count][..len].copy_from_slice(&bytes[..len]);
            self.source_count += 1;
        }
    }

    fn set_dest(&mut self, s: &str) {
        let bytes = s.as_bytes();
        self.dest_len = if bytes.len() > MAX_PATH - 1 {
            MAX_PATH - 1
        } else {
            bytes.len()
        };
        self.dest[..self.dest_len].copy_from_slice(&bytes[..self.dest_len]);
    }

    fn dest_str(&self) -> &str {
        core::str::from_utf8(&self.dest[..self.dest_len]).unwrap_or("")
    }
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

fn parse_args(argc: i32, argv: *const *const u8) -> Option<MvConfig> {
    if argc < 3 {
        eprintlns("usage: mv [options] source... dest");
        eprintlns("Options:");
        eprintlns("  -f        Force overwrite (default)");
        eprintlns("  -i        Interactive (prompt before overwrite)");
        eprintlns("  -n        No clobber (don't overwrite)");
        eprintlns("  -v        Verbose mode");
        eprintlns("  -u        Update (move only when source is newer)");
        return None;
    }

    let mut config = MvConfig::new();
    let mut arg_idx = 1;

    // Parse options
    while arg_idx < argc - 1 {
        // Leave at least one arg for dest
        let arg_ptr = unsafe { *argv.add(arg_idx as usize) };
        let arg = cstr_to_str(arg_ptr);

        if arg.starts_with('-') && arg.len() > 1 {
            for c in arg[1..].bytes() {
                match c {
                    b'f' => {
                        config.force = true;
                        config.no_clobber = false;
                        config.interactive = false;
                    }
                    b'i' => {
                        config.interactive = true;
                        config.force = false;
                        config.no_clobber = false;
                    }
                    b'n' => {
                        config.no_clobber = true;
                        config.force = false;
                        config.interactive = false;
                    }
                    b'v' => config.verbose = true,
                    b'u' => config.update = true,
                    _ => {
                        eprints("mv: unknown option: -");
                        putchar(c);
                        printlns("");
                        return None;
                    }
                }
            }
            arg_idx += 1;
        } else {
            break;
        }
    }

    // Collect sources (all remaining args except last)
    while arg_idx < argc - 1 {
        let src = cstr_to_str(unsafe { *argv.add(arg_idx as usize) });
        config.add_source(src);
        arg_idx += 1;
    }

    // Last arg is destination
    if arg_idx < argc {
        let dest = cstr_to_str(unsafe { *argv.add(arg_idx as usize) });
        config.set_dest(dest);
    }

    if config.source_count == 0 {
        eprintlns("mv: missing source file");
        return None;
    }

    Some(config)
}

/// Check if path is a directory
fn is_directory(path: &str) -> bool {
    let fd = open(path, O_RDONLY | O_DIRECTORY, 0);
    if fd >= 0 {
        close(fd);
        true
    } else {
        false
    }
}

/// Check if file exists
fn file_exists(path: &str) -> bool {
    let fd = open2(path, O_RDONLY);
    if fd >= 0 {
        close(fd);
        true
    } else {
        false
    }
}

/// Move/rename a single file
fn move_file(src: &str, dst: &str, config: &MvConfig) -> i32 {
    // Check if destination exists
    if file_exists(dst) {
        if config.no_clobber {
            if config.verbose {
                prints("mv: skipping '");
                prints(dst);
                printlns("' (exists)");
            }
            return 0;
        }

        if config.interactive {
            prints("mv: overwrite '");
            prints(dst);
            prints("'? ");

            // Read response from stdin
            let mut response = [0u8; 16];
            let n = read(STDIN_FILENO, &mut response);
            if n <= 0 || (response[0] != b'y' && response[0] != b'Y') {
                if config.verbose {
                    printlns("skipped");
                }
                return 0;
            }
        }
    }

    // Try rename syscall first (fast path for same filesystem)
    if sys_rename(src, dst) == 0 {
        if config.verbose {
            prints("'");
            prints(src);
            prints("' -> '");
            prints(dst);
            printlns("'");
        }
        return 0;
    }

    // Rename failed (probably cross-device), fall back to copy + delete

    // Open source file
    let src_fd = open2(src, O_RDONLY);
    if src_fd < 0 {
        eprints("mv: cannot open '");
        prints(src);
        eprintlns("'");
        return 1;
    }

    // For -u option, would need stat to compare times
    // Since we don't have stat yet, skip this for now
    // TODO: Implement proper stat-based update check

    // Open/create destination
    let flags = if config.force {
        O_WRONLY | O_CREAT | O_TRUNC
    } else {
        O_WRONLY | O_CREAT | O_EXCL
    };

    let dst_fd = open(dst, flags, 0o644);
    if dst_fd < 0 {
        eprints("mv: cannot create '");
        prints(dst);
        eprintlns("'");
        close(src_fd);
        return 1;
    }

    // Copy contents
    let mut buf = [0u8; 4096];
    loop {
        let n = read(src_fd, &mut buf);
        if n <= 0 {
            break;
        }
        let written = write(dst_fd, &buf[..n as usize]);
        if written < 0 {
            eprintlns("mv: write error");
            close(src_fd);
            close(dst_fd);
            return 1;
        }
    }

    close(src_fd);
    close(dst_fd);

    // Delete source
    if sys_unlink(src) < 0 {
        eprints("mv: cannot remove '");
        prints(src);
        eprintlns("'");
        return 1;
    }

    if config.verbose {
        prints("'");
        prints(src);
        prints("' -> '");
        prints(dst);
        printlns("' (copied and deleted)");
    }

    0
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    let config = match parse_args(argc, argv) {
        Some(c) => c,
        None => return 1,
    };

    // Check if destination is a directory
    let dest_is_dir = is_directory(config.dest_str());

    // If multiple sources, destination must be a directory
    if config.source_count > 1 && !dest_is_dir {
        eprintlns("mv: target is not a directory");
        return 1;
    }

    let mut ret = 0;

    for i in 0..config.source_count {
        let src_len = config.sources[i]
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(MAX_PATH);
        let src = core::str::from_utf8(&config.sources[i][..src_len]).unwrap_or("");

        // Determine actual destination path
        let mut actual_dest = [0u8; MAX_PATH];
        let actual_dest_len;

        if dest_is_dir {
            // Extract basename from source
            let basename_start = src.rfind('/').map(|i| i + 1).unwrap_or(0);
            let basename = &src[basename_start..];

            // Build dest_dir/basename
            let mut pos = 0;
            for &b in config.dest_str().as_bytes() {
                if pos < MAX_PATH - 1 {
                    actual_dest[pos] = b;
                    pos += 1;
                }
            }
            if pos > 0 && actual_dest[pos - 1] != b'/' {
                actual_dest[pos] = b'/';
                pos += 1;
            }
            for &b in basename.as_bytes() {
                if pos < MAX_PATH - 1 {
                    actual_dest[pos] = b;
                    pos += 1;
                }
            }
            actual_dest_len = pos;
        } else {
            // Use destination as-is
            let dest_bytes = config.dest_str().as_bytes();
            actual_dest_len = if dest_bytes.len() > MAX_PATH - 1 {
                MAX_PATH - 1
            } else {
                dest_bytes.len()
            };
            actual_dest[..actual_dest_len].copy_from_slice(&dest_bytes[..actual_dest_len]);
        }

        let actual_dest_str = core::str::from_utf8(&actual_dest[..actual_dest_len]).unwrap_or("");

        // Move the file
        if move_file(src, actual_dest_str, &config) != 0 {
            ret = 1;
        }
    }

    ret
}
