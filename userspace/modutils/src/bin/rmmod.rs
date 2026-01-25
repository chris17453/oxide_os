//! rmmod - Remove a kernel module
//!
//! Usage: rmmod [-f] <module>

#![no_std]
#![no_main]
#![allow(unused)]

use libc::*;

/// Syscall number for delete_module
const SYS_DELETE_MODULE: u64 = 51;

/// Module removal flags
const O_NONBLOCK: u32 = 0x800;
const O_TRUNC_FLAG: u32 = 0x200; // Different from file O_TRUNC

/// Main entry point
#[unsafe(no_mangle)]
pub fn main() -> i32 {
    // For now, just print usage since we don't have argv
    // In full implementation, we'd parse arguments

    printlns("rmmod: Remove a kernel module");
    printlns("");
    printlns("Usage: rmmod [-f] [-w] <module>");
    printlns("");
    printlns("Options:");
    printlns("  -f    Force removal (even if in use)");
    printlns("  -w    Wait for module to become unused");
    printlns("");
    printlns("Note: Full implementation requires argument passing from kernel.");
    printlns("");

    // Example implementation (commented out until argv works):
    /*
    let mut force = false;
    let mut wait = false;
    let mut module_name = "";

    for arg in &argv[1..] {
        if arg == "-f" {
            force = true;
        } else if arg == "-w" {
            wait = true;
        } else {
            module_name = arg;
        }
    }

    if module_name.is_empty() {
        eprintlns("rmmod: no module specified");
        return 1;
    }

    let flags = if force { O_TRUNC_FLAG } else { 0 }
              | if !wait { O_NONBLOCK } else { 0 };

    let ret = syscall2(SYS_DELETE_MODULE, module_name.as_ptr() as u64, flags as u64);
    if ret < 0 {
        eprints("rmmod: cannot unload ");
        eprintlns(module_name);
        return 1;
    }
    */

    0
}
