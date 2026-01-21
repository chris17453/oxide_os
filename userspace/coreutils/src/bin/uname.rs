//! uname - print system information
//!
//! Enhanced implementation with:
//! - All standard options: -a, -s, -n, -r, -v, -m, -o
//! - Multiple option support
//! - Proper formatting with spaces between fields
//! - Ready for uname syscall integration

#![no_std]
#![no_main]

use libc::*;

/// System information (utsname structure)
struct UtsName {
    sysname: &'static str,    // OS name
    nodename: &'static str,   // Network node hostname
    release: &'static str,    // OS release
    version: &'static str,    // OS version
    machine: &'static str,    // Hardware type
}

impl UtsName {
    /// Get system information
    /// TODO: Replace with actual uname() syscall when available
    fn get() -> Self {
        UtsName {
            sysname: "OXIDE",
            nodename: "localhost",
            release: "0.1.0",
            version: "#1 Mon Jan 20 2026",
            machine: "x86_64",
        }
    }
}

struct UnameConfig {
    show_sysname: bool,
    show_nodename: bool,
    show_release: bool,
    show_version: bool,
    show_machine: bool,
    show_os: bool,
}

impl UnameConfig {
    fn new() -> Self {
        UnameConfig {
            show_sysname: false,
            show_nodename: false,
            show_release: false,
            show_version: false,
            show_machine: false,
            show_os: false,
        }
    }

    /// Check if any flag is set
    fn any_set(&self) -> bool {
        self.show_sysname || self.show_nodename || self.show_release ||
        self.show_version || self.show_machine || self.show_os
    }

    /// Set all flags (for -a option)
    fn set_all(&mut self) {
        self.show_sysname = true;
        self.show_nodename = true;
        self.show_release = true;
        self.show_version = true;
        self.show_machine = true;
        self.show_os = true;
    }
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

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    let mut config = UnameConfig::new();
    let mut arg_idx = 1;

    // Parse options
    while arg_idx < argc {
        let arg_ptr = unsafe { *argv.add(arg_idx as usize) };
        let arg = cstr_to_str(arg_ptr);

        if arg.starts_with('-') && arg.len() > 1 && arg != "--" {
            // Parse character flags
            for c in arg[1..].bytes() {
                match c {
                    b'a' => config.set_all(),
                    b's' => config.show_sysname = true,
                    b'n' => config.show_nodename = true,
                    b'r' => config.show_release = true,
                    b'v' => config.show_version = true,
                    b'm' => config.show_machine = true,
                    b'o' => config.show_os = true,
                    _ => {
                        eprints("uname: unknown option: -");
                        putchar(c);
                        printlns("");
                        eprintlns("Try 'uname -a' for more information");
                        return 1;
                    }
                }
            }
            arg_idx += 1;
        } else {
            break;
        }
    }

    // If no options specified, default to -s (sysname only)
    if !config.any_set() {
        config.show_sysname = true;
    }

    // Get system information
    let utsname = UtsName::get();

    let mut first = true;

    // Print requested fields in standard order
    if config.show_sysname {
        if !first {
            putchar(b' ');
        }
        prints(utsname.sysname);
        first = false;
    }

    if config.show_nodename {
        if !first {
            putchar(b' ');
        }
        prints(utsname.nodename);
        first = false;
    }

    if config.show_release {
        if !first {
            putchar(b' ');
        }
        prints(utsname.release);
        first = false;
    }

    if config.show_version {
        if !first {
            putchar(b' ');
        }
        prints(utsname.version);
        first = false;
    }

    if config.show_machine {
        if !first {
            putchar(b' ');
        }
        prints(utsname.machine);
        first = false;
    }

    if config.show_os {
        if !first {
            putchar(b' ');
        }
        prints("OXIDE");  // Operating system name
        first = false;
    }

    printlns("");
    0
}
