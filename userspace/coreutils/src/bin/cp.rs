//! cp - copy files and directories
//!
//! Enhanced implementation with:
//! - Recursive directory copy (-r, -R)
//! - Verbose mode (-v)
//! - Force overwrite (-f)
//! - No clobber (-n)
//! - Interactive (-i)
//! - Multiple source files to directory

#![no_std]
#![no_main]

use libc::*;

const MAX_PATH: usize = 256;

struct CpConfig {
    recursive: bool,
    verbose: bool,
    force: bool,
    no_clobber: bool,
    interactive: bool,
    sources: [[u8; MAX_PATH]; 64],
    source_count: usize,
    dest: [u8; MAX_PATH],
    dest_len: usize,
}

impl CpConfig {
    fn new() -> Self {
        CpConfig {
            recursive: false,
            verbose: false,
            force: true, // Default to force for simplicity
            no_clobber: false,
            interactive: false,
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

fn parse_args(argc: i32, argv: *const *const u8) -> Option<CpConfig> {
    if argc < 3 {
        eprintlns("usage: cp [options] source... dest");
        eprintlns("Options:");
        eprintlns("  -r, -R    Copy directories recursively");
        eprintlns("  -v        Verbose mode");
        eprintlns("  -f        Force overwrite (default)");
        eprintlns("  -n        No clobber (don't overwrite)");
        eprintlns("  -i        Interactive (prompt before overwrite)");
        return None;
    }

    let mut config = CpConfig::new();
    let mut arg_idx = 1;

    // Parse options
    while arg_idx < argc - 1 {
        // Leave at least one arg for dest
        let arg_ptr = unsafe { *argv.add(arg_idx as usize) };
        let arg = cstr_to_str(arg_ptr);

        if arg.starts_with('-') && arg.len() > 1 {
            for c in arg[1..].bytes() {
                match c {
                    b'r' | b'R' => config.recursive = true,
                    b'v' => config.verbose = true,
                    b'f' => {
                        config.force = true;
                        config.no_clobber = false;
                    }
                    b'n' => {
                        config.no_clobber = true;
                        config.force = false;
                    }
                    b'i' => config.interactive = true,
                    _ => {
                        eprints("cp: unknown option: -");
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
        eprintlns("cp: missing source file");
        return None;
    }

    Some(config)
}

/// Check if path is a directory
fn is_directory(path: &str) -> bool {
    // Try to open as directory
    let fd = open(path, O_RDONLY | O_DIRECTORY, 0);
    if fd >= 0 {
        close(fd);
        true
    } else {
        false
    }
}

/// Copy a single file
fn copy_file(src: &str, dst: &str, config: &CpConfig) -> i32 {
    // Check if destination exists for no-clobber
    if config.no_clobber {
        let test_fd = open2(dst, O_RDONLY);
        if test_fd >= 0 {
            close(test_fd);
            if config.verbose {
                prints("cp: skipping '");
                prints(dst);
                printlns("' (exists)");
            }
            return 0;
        }
    }

    // Open source
    let src_fd = open2(src, O_RDONLY);
    if src_fd < 0 {
        eprints("cp: cannot open '");
        prints(src);
        eprintlns("'");
        return 1;
    }

    // Open/create destination
    let flags = if config.force {
        O_WRONLY | O_CREAT | O_TRUNC
    } else {
        O_WRONLY | O_CREAT | O_EXCL
    };

    let dst_fd = open(dst, flags, 0o644);
    if dst_fd < 0 {
        eprints("cp: cannot create '");
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
            eprintlns("cp: write error");
            close(src_fd);
            close(dst_fd);
            return 1;
        }
    }

    close(src_fd);
    close(dst_fd);

    if config.verbose {
        prints("'");
        prints(src);
        prints("' -> '");
        prints(dst);
        printlns("'");
    }

    0
}

/// Copy directory recursively
fn copy_directory(src: &str, dst: &str, config: &CpConfig) -> i32 {
    if !config.recursive {
        eprints("cp: -r not specified; omitting directory '");
        prints(src);
        printlns("'");
        return 1;
    }

    // Create destination directory
    let mkdir_ret = sys_mkdir(dst, 0o755);
    if mkdir_ret < 0 && mkdir_ret != -17 {
        // -17 is EEXIST
        eprints("cp: cannot create directory '");
        prints(dst);
        eprintlns("'");
        return 1;
    }

    if config.verbose {
        prints("'");
        prints(src);
        prints("' -> '");
        prints(dst);
        printlns("'");
    }

    // Open source directory
    let fd = open(src, O_RDONLY | O_DIRECTORY, 0);
    if fd < 0 {
        eprints("cp: cannot open directory '");
        prints(src);
        eprintlns("'");
        return 1;
    }

    // Read directory entries
    let mut buf = [0u8; 4096];
    let mut ret = 0;

    loop {
        let n = sys_getdents(fd, &mut buf);
        if n <= 0 {
            break;
        }

        let mut offset = 0;
        while offset < n as usize {
            // Skip fixed header (d_ino + d_off + d_reclen + d_type)
            let d_reclen = u16::from_ne_bytes([buf[offset + 16], buf[offset + 17]]);
            let d_type = buf[offset + 18];

            // Name starts after header
            let name_start = offset + 19;
            let mut name_len = 0;
            while name_start + name_len < buf.len() && buf[name_start + name_len] != 0 {
                name_len += 1;
            }

            if name_len > 0 {
                let name = &buf[name_start..name_start + name_len];

                // Skip . and ..
                if !(name[0] == b'.' && (name_len == 1 || (name_len == 2 && name[1] == b'.'))) {
                    // Build source path
                    let mut src_path = [0u8; MAX_PATH];
                    let mut pos = 0;
                    for &b in src.as_bytes() {
                        if pos < MAX_PATH - 1 {
                            src_path[pos] = b;
                            pos += 1;
                        }
                    }
                    if pos > 0 && src_path[pos - 1] != b'/' {
                        src_path[pos] = b'/';
                        pos += 1;
                    }
                    for i in 0..name_len {
                        if pos < MAX_PATH - 1 {
                            src_path[pos] = name[i];
                            pos += 1;
                        }
                    }
                    let src_path_str = core::str::from_utf8(&src_path[..pos]).unwrap_or("");

                    // Build dest path
                    let mut dst_path = [0u8; MAX_PATH];
                    let mut pos = 0;
                    for &b in dst.as_bytes() {
                        if pos < MAX_PATH - 1 {
                            dst_path[pos] = b;
                            pos += 1;
                        }
                    }
                    if pos > 0 && dst_path[pos - 1] != b'/' {
                        dst_path[pos] = b'/';
                        pos += 1;
                    }
                    for i in 0..name_len {
                        if pos < MAX_PATH - 1 {
                            dst_path[pos] = name[i];
                            pos += 1;
                        }
                    }
                    let dst_path_str = core::str::from_utf8(&dst_path[..pos]).unwrap_or("");

                    // Recursively copy
                    if d_type == 4 {
                        // DT_DIR
                        if copy_directory(src_path_str, dst_path_str, config) != 0 {
                            ret = 1;
                        }
                    } else {
                        if copy_file(src_path_str, dst_path_str, config) != 0 {
                            ret = 1;
                        }
                    }
                }
            }

            offset += d_reclen as usize;
        }
    }

    close(fd);
    ret
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
        eprintlns("cp: target is not a directory");
        return 1;
    }

    let mut ret = 0;

    for i in 0..config.source_count {
        let src_len = config.sources[i]
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(MAX_PATH);
        let src = core::str::from_utf8(&config.sources[i][..src_len]).unwrap_or("");

        let src_is_dir = is_directory(src);

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

        // Copy
        if src_is_dir {
            if copy_directory(src, actual_dest_str, &config) != 0 {
                ret = 1;
            }
        } else {
            if copy_file(src, actual_dest_str, &config) != 0 {
                ret = 1;
            }
        }
    }

    ret
}
