#![no_std]
#![no_main]

use libc::*;

const MAX_INPUT: usize = 256;

fn read_line(buf: &mut [u8], echo: bool) -> usize {
    let mut i = 0;
    loop {
        let mut c = [0u8; 1];
        if read(0, &mut c) <= 0 {
            break;
        }
        match c[0] {
            b'\n' | b'\r' => {
                if echo {
                    prints("\n");
                }
                break;
            }
            127 | 8 => {
                if i > 0 {
                    i -= 1;
                    if echo {
                        prints("\x08 \x08");
                    }
                }
            }
            _ => {
                if i < buf.len() - 1 {
                    buf[i] = c[0];
                    i += 1;
                    if echo {
                        let s = [c[0]];
                        if let Ok(ch) = core::str::from_utf8(&s) {
                            prints(ch);
                        }
                    }
                }
            }
        }
    }
    buf[i] = 0;
    i
}

fn str_eq(a: &[u8], b: &str) -> bool {
    let b_bytes = b.as_bytes();
    let a_len = a.iter().position(|&c| c == 0).unwrap_or(a.len());
    if a_len != b_bytes.len() {
        return false;
    }
    for i in 0..a_len {
        if a[i] != b_bytes[i] {
            return false;
        }
    }
    true
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    // Only allow root for now
    if getuid() != 0 {
        eprintlns("passwd: only root can change password (session only)");
        return 1;
    }

    // Prompt for new password twice
    prints("New password: ");
    let mut pw1 = [0u8; MAX_INPUT];
    let l1 = read_line(&mut pw1, false);
    prints("\n");

    if l1 == 0 {
        eprintlns("passwd: empty password not allowed");
        return 1;
    }

    prints("Retype new password: ");
    let mut pw2 = [0u8; MAX_INPUT];
    let l2 = read_line(&mut pw2, false);
    prints("\n");

    if l1 != l2
        || !str_eq(
            &pw1[..l1],
            core::str::from_utf8(&pw2[..l2]).unwrap_or_default(),
        )
    {
        eprintlns("passwd: passwords do not match");
        return 1;
    }

    unsafe extern "C" {
        fn login_set_root_password(ptr: *const u8, len: usize);
    }
    unsafe {
        login_set_root_password(pw1.as_ptr(), l1);
    }

    printlns("Password updated (session only)");
    0
}
