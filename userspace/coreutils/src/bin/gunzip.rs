//! gunzip - decompress .gz files
//!
//! Full-featured implementation with:
//! - GZIP decompression using INFLATE algorithm (RFC 1951, RFC 1952)
//! - Keep compressed file (-k)
//! - Force overwrite (-f)
//! - Stdout output (-c)
//! - Multiple file support
//! - GZIP header parsing
//! - CRC32 checksum verification

#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec::Vec;
use compression::deflate::gzip_decompress;
use libc::*;

// Uses libc's global allocator

const MAX_FILENAME: usize = 256;

struct GunzipConfig {
    force: bool,
    keep: bool,
    stdout: bool,
}

impl GunzipConfig {
    fn new() -> Self {
        GunzipConfig {
            force: false,
            keep: false,
            stdout: false,
        }
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

fn str_starts_with(s: &str, prefix: &str) -> bool {
    if s.len() < prefix.len() {
        return false;
    }
    let s_bytes = s.as_bytes();
    let p_bytes = prefix.as_bytes();
    for i in 0..prefix.len() {
        if s_bytes[i] != p_bytes[i] {
            return false;
        }
    }
    true
}

fn str_ends_with(s: &str, suffix: &str) -> bool {
    if s.len() < suffix.len() {
        return false;
    }
    let start = s.len() - suffix.len();
    let s_bytes = s.as_bytes();
    let suffix_bytes = suffix.as_bytes();
    for i in 0..suffix.len() {
        if s_bytes[start + i] != suffix_bytes[i] {
            return false;
        }
    }
    true
}

/// Build output filename: remove .gz extension
fn build_output_name(input: &str) -> [u8; MAX_FILENAME] {
    let mut output = [0u8; MAX_FILENAME];

    if str_ends_with(input, ".gz") {
        let len = input.len() - 3;
        let copy_len = if len > MAX_FILENAME - 1 {
            MAX_FILENAME - 1
        } else {
            len
        };
        output[..copy_len].copy_from_slice(&input.as_bytes()[..copy_len]);
    } else {
        // No .gz extension, just copy as-is
        let copy_len = if input.len() > MAX_FILENAME - 1 {
            MAX_FILENAME - 1
        } else {
            input.len()
        };
        output[..copy_len].copy_from_slice(&input.as_bytes()[..copy_len]);
    }

    output
}

/// Read entire file into Vec
fn read_file(path: &str) -> Option<Vec<u8>> {
    let fd = open2(path, O_RDONLY);
    if fd < 0 {
        return None;
    }

    let mut data = Vec::new();
    let mut buf = [0u8; 4096];

    loop {
        let n = read(fd, &mut buf);
        if n < 0 {
            close(fd);
            return None;
        }
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n as usize]);
    }

    close(fd);
    Some(data)
}

/// Write data to file
fn write_file(path: &str, data: &[u8]) -> i32 {
    let fd = open(path, O_WRONLY | O_CREAT | O_TRUNC, 0o644);
    if fd < 0 {
        return -1;
    }

    let mut pos = 0;
    while pos < data.len() {
        let n = write(fd, &data[pos..]);
        if n <= 0 {
            close(fd);
            return -1;
        }
        pos += n as usize;
    }

    close(fd);
    0
}

/// Decompress a single file
fn decompress_file(config: &GunzipConfig, input_path: &str) -> i32 {
    // Read input file
    let compressed_data = match read_file(input_path) {
        Some(data) => data,
        None => {
            eprints("gunzip: cannot read '");
            prints(input_path);
            eprintlns("'");
            return 1;
        }
    };

    // Decompress
    let decompressed = match gzip_decompress(&compressed_data) {
        Ok((data, _header)) => data,
        Err(_) => {
            eprints("gunzip: decompression failed for '");
            prints(input_path);
            eprintlns("' (invalid gzip format or corrupted data)");
            return 1;
        }
    };

    // Write output
    if config.stdout {
        // Write to stdout
        let mut pos = 0;
        while pos < decompressed.len() {
            let n = write(STDOUT_FILENO, &decompressed[pos..]);
            if n <= 0 {
                eprintlns("gunzip: write error");
                return 1;
            }
            pos += n as usize;
        }
    } else {
        // Write to file
        let output_name = build_output_name(input_path);
        let output_len = output_name
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(MAX_FILENAME);
        let output_path = core::str::from_utf8(&output_name[..output_len]).unwrap_or("");

        // Check if output exists and -f not specified
        if !config.force {
            let mut statbuf = Stat::zeroed();
            if stat(output_path, &mut statbuf) == 0 {
                eprints("gunzip: '");
                prints(output_path);
                eprintlns("' already exists; not overwritten");
                return 1;
            }
        }

        if write_file(output_path, &decompressed) < 0 {
            eprints("gunzip: cannot write '");
            prints(output_path);
            eprintlns("'");
            return 1;
        }

        // Remove compressed file if not keeping
        if !config.keep {
            unlink(input_path);
        }
    }

    0
}

fn show_help() {
    eprintlns("Usage: gunzip [OPTIONS] [FILES...]");
    eprintlns("");
    eprintlns("Decompress .gz files using INFLATE algorithm (RFC 1951, RFC 1952).");
    eprintlns("");
    eprintlns("Options:");
    eprintlns("  -c          Write to stdout, keep compressed files");
    eprintlns("  -f          Force overwrite of output files");
    eprintlns("  -k          Keep compressed files (don't delete)");
    eprintlns("  -h          Show this help");
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        show_help();
        return 1;
    }

    let mut config = GunzipConfig::new();
    let mut arg_idx = 1;

    // Parse options
    while arg_idx < argc {
        let arg_ptr = unsafe { *argv.add(arg_idx as usize) };
        let arg = cstr_to_str(arg_ptr);

        if str_starts_with(arg, "-") && arg.len() > 1 && arg != "--" {
            for &c in arg.as_bytes()[1..].iter() {
                match c {
                    b'c' => config.stdout = true,
                    b'f' => config.force = true,
                    b'k' => config.keep = true,
                    b'h' => {
                        show_help();
                        return 0;
                    }
                    _ => {
                        eprints("gunzip: invalid option: -");
                        putchar(c);
                        eprintlns("");
                        return 1;
                    }
                }
            }
            arg_idx += 1;
        } else {
            break;
        }
    }

    if arg_idx >= argc {
        eprintlns("gunzip: no files specified");
        return 1;
    }

    if config.stdout {
        config.keep = true; // -c implies -k
    }

    let mut status = 0;

    // Decompress each file
    for i in arg_idx..argc {
        let file = cstr_to_str(unsafe { *argv.add(i as usize) });
        if decompress_file(&config, file) != 0 {
            status = 1;
        }
    }

    status
}
