//! umount - Unmount a filesystem
//!
//! Usage: umount [-f] [-l] target
//!        umount -a              (unmount all)
//!
//! Options:
//!   -f, --force     Force unmount (even if busy)
//!   -l, --lazy      Lazy unmount (detach but keep using)
//!   -a, --all       Unmount all filesystems in /etc/mtab
//!   -v, --verbose   Verbose output
//!   --help          Show this help message

#![no_std]
#![no_main]

use libc::println;
use libc::syscall::umount_flags::*;

/// Parse command line arguments
struct Args {
    targets: [Option<&'static str>; 16],
    target_count: usize,
    flags: u32,
    unmount_all: bool,
    show_help: bool,
    verbose: bool,
}

impl Args {
    fn new() -> Self {
        Args {
            targets: [None; 16],
            target_count: 0,
            flags: 0,
            unmount_all: false,
            show_help: false,
            verbose: false,
        }
    }

    fn add_target(&mut self, target: &'static str) {
        if self.target_count < 16 {
            self.targets[self.target_count] = Some(target);
            self.target_count += 1;
        }
    }
}

fn show_help() {
    println!("Usage: umount [-f] [-l] target...");
    println!("       umount -a            Unmount all");
    println!();
    println!("Options:");
    println!("  -f, --force     Force unmount even if busy");
    println!("  -l, --lazy      Lazy unmount (detach from tree)");
    println!("  -a, --all       Unmount all filesystems");
    println!("  -v, --verbose   Verbose output");
    println!("  --help          Show this help message");
}

/// Unmount all filesystems (except root)
fn unmount_all(verbose: bool) -> i32 {
    // Read current mounts from /proc/mounts
    let fd = libc::open("/proc/mounts", libc::O_RDONLY, 0);
    if fd < 0 {
        println!("umount: cannot read /proc/mounts");
        return 1;
    }

    let mut buf = [0u8; 4096];
    let n = libc::read(fd, &mut buf);
    libc::close(fd);

    if n <= 0 {
        return 0;
    }

    let data = unsafe { core::str::from_utf8_unchecked(&buf[..n as usize]) };

    // Collect mount points (skip root)
    let mut mount_points: [Option<&str>; 32] = [None; 32];
    let mut count = 0;

    for line in data.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Format: device mountpoint fstype options ...
        let mut parts = line.split_whitespace();
        let _device = parts.next();
        let mountpoint = parts.next();

        if let Some(mp) = mountpoint {
            // Skip root and essential filesystems
            if mp != "/" && mp != "/dev" && mp != "/proc" && mp != "/sys" {
                if count < 32 {
                    mount_points[count] = Some(mp);
                    count += 1;
                }
            }
        }
    }

    // Unmount in reverse order (deepest paths first)
    let mut errors = 0;
    for i in (0..count).rev() {
        if let Some(mp) = mount_points[i] {
            if verbose {
                println!("umount: unmounting {}", mp);
            }

            let result = libc::syscall::umount(mp);
            if result < 0 {
                println!("umount: {}: failed (error {})", mp, -result);
                errors += 1;
            }
        }
    }

    errors
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    let mut args = Args::new();

    // Parse arguments
    let mut i = 1;
    while i < argc as usize {
        let arg = unsafe {
            let ptr = *argv.add(i);
            let mut len = 0;
            while *ptr.add(len) != 0 {
                len += 1;
            }
            core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr, len))
        };

        if arg == "-h" || arg == "--help" {
            args.show_help = true;
        } else if arg == "-a" || arg == "--all" {
            args.unmount_all = true;
        } else if arg == "-f" || arg == "--force" {
            args.flags |= MNT_FORCE;
        } else if arg == "-l" || arg == "--lazy" {
            args.flags |= MNT_DETACH;
        } else if arg == "-v" || arg == "--verbose" {
            args.verbose = true;
        } else if !arg.starts_with('-') {
            // Target mount point
            args.add_target(arg);
        } else {
            println!("umount: unknown option: {}", arg);
            return 1;
        }

        i += 1;
    }

    // Handle --help
    if args.show_help {
        show_help();
        return 0;
    }

    // Handle -a (unmount all)
    if args.unmount_all {
        return unmount_all(args.verbose);
    }

    // Need at least one target
    if args.target_count == 0 {
        println!("umount: missing target");
        println!("Usage: umount [-f] [-l] target");
        return 1;
    }

    // Unmount each target
    let mut errors = 0;
    for i in 0..args.target_count {
        if let Some(target) = args.targets[i] {
            if args.verbose {
                println!("umount: unmounting {}", target);
            }

            let result = libc::syscall::umount2(target, args.flags);

            if result < 0 {
                let err = -result;
                let msg = match err {
                    1 => "operation not permitted",
                    2 => "no such file or directory",
                    16 => "device or resource busy",
                    22 => "invalid argument (not mounted?)",
                    _ => "unknown error",
                };
                println!("umount: {}: {} ({})", target, msg, err);
                errors += 1;
            } else if args.verbose {
                println!("umount: {} unmounted", target);
            }
        }
    }

    errors
}
