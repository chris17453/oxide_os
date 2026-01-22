//! id - print user and group IDs

#![no_std]
#![no_main]

use libc::pwd::{getegid, geteuid, getgid, getuid};
use libc::*;

#[unsafe(no_mangle)]
fn main() -> i32 {
    let uid = getuid();
    let gid = getgid();
    let euid = geteuid();
    let egid = getegid();

    prints("uid=");
    print_u64(uid as u64);

    if uid == 0 {
        prints("(root)");
    }

    prints(" gid=");
    print_u64(gid as u64);

    if gid == 0 {
        prints("(root)");
    }

    if euid != uid {
        prints(" euid=");
        print_u64(euid as u64);
    }

    if egid != gid {
        prints(" egid=");
        print_u64(egid as u64);
    }

    printlns("");

    0
}
