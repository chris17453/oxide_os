//! modinfo - Show information about a kernel module
//!
//! Usage: modinfo <module.ko>

#![no_std]
#![no_main]

use libc::*;

/// Main entry point
#[unsafe(no_mangle)]
pub fn main() -> i32 {
    // For now, just print usage since we don't have argv
    // In full implementation, we'd parse the .modinfo section of the .ko file

    printlns("modinfo: Show information about a kernel module");
    printlns("");
    printlns("Usage: modinfo <module.ko>");
    printlns("");
    printlns("Displays:");
    printlns("  filename    - Path to module file");
    printlns("  name        - Module name");
    printlns("  version     - Module version");
    printlns("  author      - Module author");
    printlns("  description - Module description");
    printlns("  license     - Module license");
    printlns("  depends     - Module dependencies");
    printlns("");
    printlns("Note: Full implementation requires argument passing from kernel.");
    printlns("");

    // Example implementation (commented out until argv works):
    /*
    let path = argv[1];

    // Open and read the .ko file
    let fd = open(path, O_RDONLY);
    if fd < 0 {
        eprints("modinfo: cannot open ");
        eprintlns(path);
        return 1;
    }

    // Parse ELF and find .modinfo section
    // The .modinfo section contains key=value pairs separated by nulls

    // Print info
    prints("filename:       ");
    printlns(path);
    // ... parse and print other fields

    close(fd);
    */

    0
}
