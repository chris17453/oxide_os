//! id - print user and group IDs

#![no_std]
#![no_main]

use libc::*;
use libc::pwd::{getuid, geteuid, getgid, getegid};

#[unsafe(no_mangle)]
fn main() -> i32 {
    let uid = getuid();
    let gid = getgid();
    let euid = geteuid();
    let egid = getegid();

    print("uid=");
    print_u64(uid as u64);

    if uid == 0 {
        print("(root)");
    }

    print(" gid=");
    print_u64(gid as u64);

    if gid == 0 {
        print("(root)");
    }

    if euid != uid {
        print(" euid=");
        print_u64(euid as u64);
    }

    if egid != gid {
        print(" egid=");
        print_u64(egid as u64);
    }

    println("");

    0
}
