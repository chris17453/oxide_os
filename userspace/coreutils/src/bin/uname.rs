//! uname - print system information

#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
fn main() -> i32 {
    printlns("OXIDE");
    0
}
