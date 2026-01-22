//! dd - convert and copy a file

#![no_std]
#![no_main]

use libc::*;

const DEFAULT_BLOCK_SIZE: usize = 512;

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    let mut input_file: Option<&str> = None;
    let mut output_file: Option<&str> = None;
    let mut block_size = DEFAULT_BLOCK_SIZE;
    let mut count: Option<usize> = None;
    let mut skip: usize = 0;
    let mut seek: usize = 0;

    // Parse arguments (format: dd if=input of=output bs=512 count=10)
    for i in 1..argc {
        let arg = unsafe { cstr_to_str(*argv.add(i as usize)) };

        if let Some(val) = arg.strip_prefix("if=") {
            input_file = Some(val);
        } else if let Some(val) = arg.strip_prefix("of=") {
            output_file = Some(val);
        } else if let Some(val) = arg.strip_prefix("bs=") {
            block_size = match parse_int(val.as_bytes()) {
                Some(v) => v as usize,
                None => {
                    eprintlns("dd: invalid block size");
                    return 1;
                }
            };
        } else if let Some(val) = arg.strip_prefix("count=") {
            count = match parse_int(val.as_bytes()) {
                Some(v) => Some(v as usize),
                None => {
                    eprintlns("dd: invalid count");
                    return 1;
                }
            };
        } else if let Some(val) = arg.strip_prefix("skip=") {
            skip = match parse_int(val.as_bytes()) {
                Some(v) => v as usize,
                None => {
                    eprintlns("dd: invalid skip");
                    return 1;
                }
            };
        } else if let Some(val) = arg.strip_prefix("seek=") {
            seek = match parse_int(val.as_bytes()) {
                Some(v) => v as usize,
                None => {
                    eprintlns("dd: invalid seek");
                    return 1;
                }
            };
        }
    }

    // Open input (default: stdin)
    let in_fd = if let Some(path) = input_file {
        let fd = open2(path, O_RDONLY);
        if fd < 0 {
            eprints("dd: cannot open input file '");
            eprints(path);
            eprintlns("'");
            return 1;
        }

        // Skip blocks if requested
        if skip > 0 {
            lseek(fd, (skip * block_size) as i64, SEEK_SET);
        }

        fd
    } else {
        STDIN_FILENO
    };

    // Open output (default: stdout)
    let out_fd = if let Some(path) = output_file {
        let fd = open2(path, O_WRONLY | O_CREAT | O_TRUNC);
        if fd < 0 {
            eprints("dd: cannot open output file '");
            eprints(path);
            eprintlns("'");
            if in_fd != STDIN_FILENO {
                close(in_fd);
            }
            return 1;
        }

        // Seek if requested
        if seek > 0 {
            lseek(fd, (seek * block_size) as i64, SEEK_SET);
        }

        fd
    } else {
        STDOUT_FILENO
    };

    // Allocate buffer on stack (limited size)
    let mut buffer = [0u8; 4096];
    let buf_size = if block_size > 4096 { 4096 } else { block_size };

    let mut blocks_copied = 0usize;
    let mut bytes_copied = 0usize;

    loop {
        // Check if we've reached count limit
        if let Some(max_count) = count {
            if blocks_copied >= max_count {
                break;
            }
        }

        // Read block
        let n = read(in_fd, &mut buffer[..buf_size]);
        if n <= 0 {
            break;
        }

        // Write block
        let written = write(out_fd, &buffer[..n as usize]);
        if written != n {
            eprintlns("dd: write error");
            break;
        }

        blocks_copied += 1;
        bytes_copied += n as usize;
    }

    // Close files if not stdin/stdout
    if in_fd != STDIN_FILENO {
        close(in_fd);
    }
    if out_fd != STDOUT_FILENO {
        close(out_fd);
    }

    // Print statistics
    print_u64(blocks_copied as u64);
    prints("+0 records in\n");
    print_u64(blocks_copied as u64);
    prints("+0 records out\n");
    print_u64(bytes_copied as u64);
    printlns(" bytes copied");

    0
}

fn parse_int(s: &[u8]) -> Option<i64> {
    if s.is_empty() {
        return None;
    }

    let mut result: i64 = 0;
    for &c in s {
        if c < b'0' || c > b'9' {
            return None;
        }
        result = result * 10 + (c - b'0') as i64;
    }

    Some(result)
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

trait StrExt {
    fn strip_prefix(&self, prefix: &str) -> Option<&str>;
}

impl StrExt for &str {
    fn strip_prefix(&self, prefix: &str) -> Option<&str> {
        if self.starts_with(prefix) {
            Some(&self[prefix.len()..])
        } else {
            None
        }
    }
}
