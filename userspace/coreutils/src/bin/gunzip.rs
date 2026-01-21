//! gunzip - decompress .gz files

#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        eprintlns("usage: gunzip <file.gz>");
        return 1;
    }

    let mut force = false;
    let mut keep = false;
    let mut file_idx = 1;

    // Parse options
    let arg1 = unsafe { cstr_to_str(*argv.add(1)) };
    if arg1.starts_with('-') {
        for &c in arg1.as_bytes()[1..].iter() {
            match c {
                b'f' => force = true,
                b'k' => keep = true,
                _ => {
                    eprints("gunzip: invalid option: ");
                    putchar(c);
                    eprintlns("");
                    return 1;
                }
            }
        }
        file_idx = 2;
    }

    if file_idx >= argc {
        eprintlns("gunzip: no file specified");
        return 1;
    }

    let file = unsafe { cstr_to_str(*argv.add(file_idx as usize)) };

    prints("gunzip: would decompress '");
    prints(file);
    printlns("'");

    if force {
        printlns("  (force mode)");
    }
    if keep {
        printlns("  (keep compressed file)");
    }

    // In a full implementation, this would:
    // 1. Open .gz file
    // 2. Read and verify gzip header
    // 3. Apply INFLATE decompression algorithm
    // 4. Write decompressed data to output file
    // 5. Optionally remove .gz file

    eprintlns("gunzip: decompression not yet implemented");
    eprintlns("  (requires INFLATE algorithm implementation)");

    1
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
    fn starts_with(&self, prefix: &str) -> bool;
}

impl StrExt for &str {
    fn starts_with(&self, prefix: &str) -> bool {
        if self.len() < prefix.len() {
            return false;
        }
        let self_bytes = self.as_bytes();
        let prefix_bytes = prefix.as_bytes();
        for i in 0..prefix.len() {
            if self_bytes[i] != prefix_bytes[i] {
                return false;
            }
        }
        true
    }
}
