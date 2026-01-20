//! whoami - print effective user name

#![no_std]
#![no_main]

use libc::*;
use libc::pwd::geteuid;

#[unsafe(no_mangle)]
fn main() -> i32 {
    let euid = geteuid();

    if euid == 0 {
        printlns("root");
    } else {
        // For now, just print the UID if not root
        prints("user");
        print_u64(euid as u64);
        printlns("");
    }

    0
}
