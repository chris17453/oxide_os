//! file - determine file type
//!
//! Determine type of FILEs.

#![no_std]
#![no_main]

use libc::*;

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

// Magic bytes for file identification
const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];
const PNG_MAGIC: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
const JPEG_MAGIC: [u8; 2] = [0xff, 0xd8];
const GIF_MAGIC: [u8; 4] = [b'G', b'I', b'F', b'8'];
const PDF_MAGIC: [u8; 4] = [b'%', b'P', b'D', b'F'];
const ZIP_MAGIC: [u8; 2] = [b'P', b'K'];
const GZIP_MAGIC: [u8; 2] = [0x1f, 0x8b];
const TAR_MAGIC_OFFSET: usize = 257;
const TAR_MAGIC: [u8; 5] = [b'u', b's', b't', b'a', b'r'];
const CPIO_MAGIC: [u8; 6] = [b'0', b'7', b'0', b'7', b'0', b'1'];

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        eprintlns("usage: file file...");
        return 1;
    }

    for i in 1..argc {
        let filename = ptr_to_str(unsafe { *argv.add(i as usize) });
        prints(filename);
        prints(": ");
        identify_file(filename);
        printlns("");
    }

    0
}

fn identify_file(filename: &str) {
    // First, stat the file
    let mut st = Stat::zeroed();
    if stat(filename, &mut st) < 0 {
        prints("cannot open");
        return;
    }

    // Check file type from stat
    let mode = st.mode & S_IFMT;
    match mode {
        m if m == S_IFDIR => {
            prints("directory");
            return;
        }
        m if m == S_IFLNK => {
            prints("symbolic link");
            return;
        }
        m if m == S_IFCHR => {
            prints("character special");
            return;
        }
        m if m == S_IFBLK => {
            prints("block special");
            return;
        }
        m if m == S_IFIFO => {
            prints("fifo (named pipe)");
            return;
        }
        m if m == S_IFSOCK => {
            prints("socket");
            return;
        }
        _ => {}
    }

    // It's a regular file - read magic bytes
    let fd = open(filename, O_RDONLY, 0);
    if fd < 0 {
        prints("cannot open");
        return;
    }

    let mut buf = [0u8; 512];
    let n = read(fd, &mut buf);
    close(fd);

    if n <= 0 {
        prints("empty");
        return;
    }

    let count = n as usize;

    // Check for various file types by magic bytes
    if count >= 4 && buf[..4] == ELF_MAGIC {
        prints("ELF ");
        if count > 4 {
            match buf[4] {
                1 => prints("32-bit "),
                2 => prints("64-bit "),
                _ => {}
            }
        }
        if count > 5 {
            match buf[5] {
                1 => prints("LSB "),
                2 => prints("MSB "),
                _ => {}
            }
        }
        if count > 16 {
            match (buf[16] as u16) | ((buf[17] as u16) << 8) {
                1 => prints("relocatable"),
                2 => prints("executable"),
                3 => prints("shared object"),
                4 => prints("core file"),
                _ => prints("object"),
            }
        }
        return;
    }

    if count >= 8 && buf[..8] == PNG_MAGIC {
        prints("PNG image data");
        return;
    }

    if count >= 2 && buf[..2] == JPEG_MAGIC {
        prints("JPEG image data");
        return;
    }

    if count >= 4 && buf[..4] == GIF_MAGIC {
        prints("GIF image data");
        return;
    }

    if count >= 4 && buf[..4] == PDF_MAGIC {
        prints("PDF document");
        return;
    }

    if count >= 2 && buf[..2] == ZIP_MAGIC {
        prints("Zip archive data");
        return;
    }

    if count >= 2 && buf[..2] == GZIP_MAGIC {
        prints("gzip compressed data");
        return;
    }

    if count > TAR_MAGIC_OFFSET + 5 && buf[TAR_MAGIC_OFFSET..TAR_MAGIC_OFFSET + 5] == TAR_MAGIC {
        prints("POSIX tar archive");
        return;
    }

    if count >= 6 && buf[..6] == CPIO_MAGIC {
        prints("ASCII cpio archive");
        return;
    }

    // Check for shell script
    if count >= 2 && buf[0] == b'#' && buf[1] == b'!' {
        prints("script, ");
        // Find end of first line
        let mut end = 2;
        while end < count && buf[end] != b'\n' {
            end += 1;
        }
        // Print interpreter
        for i in 2..end.min(64) {
            if buf[i] == b' ' || buf[i] == b'\n' {
                break;
            }
            putchar(buf[i]);
        }
        prints(" script");
        return;
    }

    // Check if it's text
    let mut is_text = true;
    let mut has_printable = false;
    for i in 0..count {
        let c = buf[i];
        if c == 0 {
            is_text = false;
            break;
        }
        if c >= 0x20 && c < 0x7f {
            has_printable = true;
        } else if c != b'\n' && c != b'\r' && c != b'\t' {
            is_text = false;
            break;
        }
    }

    if is_text && has_printable {
        prints("ASCII text");
        return;
    }

    prints("data");
}
