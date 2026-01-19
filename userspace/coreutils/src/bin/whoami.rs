//! whoami - print effective user name

#![no_std]
#![no_main]

use efflux_libc::*;
use efflux_libc::pwd::geteuid;

#[unsafe(no_mangle)]
fn main() -> i32 {
    let euid = geteuid();

    if euid == 0 {
        println("root");
    } else {
        // For now, just print the UID if not root
        print("user");
        print_u64(euid as u64);
        println("");
    }

    0
}
