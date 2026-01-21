//! diff - compare files line by line

#![no_std]
#![no_main]

use libc::*;

const MAX_LINE_LEN: usize = 4096;

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 3 {
        eprintlns("usage: diff <file1> <file2>");
        return 1;
    }

    let file1 = unsafe { cstr_to_str(*argv.add(1)) };
    let file2 = unsafe { cstr_to_str(*argv.add(2)) };

    // Open both files
    let fd1 = open2(file1, O_RDONLY);
    if fd1 < 0 {
        eprints("diff: cannot open '");
        eprints(file1);
        eprintlns("'");
        return 1;
    }

    let fd2 = open2(file2, O_RDONLY);
    if fd2 < 0 {
        eprints("diff: cannot open '");
        eprints(file2);
        eprintlns("'");
        close(fd1);
        return 1;
    }

    let mut line1 = [0u8; MAX_LINE_LEN];
    let mut line2 = [0u8; MAX_LINE_LEN];
    let mut line_num = 1;
    let mut differences = 0;

    loop {
        let len1 = read_line(fd1, &mut line1);
        let len2 = read_line(fd2, &mut line2);

        // Both files ended
        if len1 == 0 && len2 == 0 {
            break;
        }

        // One file ended before the other
        if len1 == 0 || len2 == 0 || len1 != len2 || !lines_equal(&line1[..len1], &line2[..len2]) {
            differences += 1;
            
            print_i64(line_num as i64);
            printlns(":");
            
            if len1 > 0 {
                prints("< ");
                for i in 0..len1 {
                    putchar(line1[i]);
                }
                if len1 > 0 && line1[len1 - 1] != b'\n' {
                    printlns("");
                }
            }
            
            if len2 > 0 {
                prints("> ");
                for i in 0..len2 {
                    putchar(line2[i]);
                }
                if len2 > 0 && line2[len2 - 1] != b'\n' {
                    printlns("");
                }
            }
        }

        if len1 == 0 || len2 == 0 {
            break;
        }

        line_num += 1;
    }

    close(fd1);
    close(fd2);

    if differences > 0 {
        1 // Files differ
    } else {
        0 // Files are identical
    }
}

fn read_line(fd: i32, buf: &mut [u8]) -> usize {
    let mut pos = 0;
    let mut temp = [0u8; 1];
    
    while pos < buf.len() {
        let n = read(fd, &mut temp);
        if n <= 0 {
            break;
        }
        
        buf[pos] = temp[0];
        pos += 1;
        
        if temp[0] == b'\n' {
            break;
        }
    }
    
    pos
}

fn lines_equal(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    
    for i in 0..a.len() {
        if a[i] != b[i] {
            return false;
        }
    }
    
    true
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
