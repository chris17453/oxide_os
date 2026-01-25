//! mount - Mount a filesystem
//!
//! Usage: mount [-t type] [-o options] source target
//!        mount                    (lists all mounts)
//!        mount -a                 (mount all from /etc/fstab)
//!
//! Options:
//!   -t type     Filesystem type (ext4, tmpfs, etc.)
//!   -o options  Mount options (ro, rw, noatime, etc.)
//!   -a          Mount all filesystems in /etc/fstab
//!   -r          Mount read-only (same as -o ro)
//!   -w          Mount read-write (same as -o rw)
//!   -v          Verbose output
//!   --help      Show this help message

#![no_std]
#![no_main]

use core::ptr;
use libc::syscall::mount_flags::*;
use libc::{print, println};

/// Parse command line arguments
struct Args {
    source: Option<&'static str>,
    target: Option<&'static str>,
    fstype: Option<&'static str>,
    flags: u32,
    mount_all: bool,
    show_help: bool,
    verbose: bool,
    list_mounts: bool,
}

impl Args {
    fn new() -> Self {
        Args {
            source: None,
            target: None,
            fstype: None,
            flags: 0,
            mount_all: false,
            show_help: false,
            verbose: false,
            list_mounts: false,
        }
    }
}

/// Parse mount options string (e.g., "ro,noatime,nosuid")
fn parse_options(opts: &str) -> u32 {
    let mut flags = 0u32;

    for opt in opts.split(',') {
        let opt = opt.trim();
        match opt {
            "ro" | "rdonly" => flags |= MS_RDONLY,
            "rw" => flags &= !MS_RDONLY,
            "nosuid" => flags |= MS_NOSUID,
            "suid" => flags &= !MS_NOSUID,
            "nodev" => flags |= MS_NODEV,
            "dev" => flags &= !MS_NODEV,
            "noexec" => flags |= MS_NOEXEC,
            "exec" => flags &= !MS_NOEXEC,
            "sync" => flags |= MS_SYNCHRONOUS,
            "async" => flags &= !MS_SYNCHRONOUS,
            "remount" => flags |= MS_REMOUNT,
            "noatime" => flags |= MS_NOATIME,
            "atime" => flags &= !MS_NOATIME,
            "nodiratime" => flags |= MS_NODIRATIME,
            "diratime" => flags &= !MS_NODIRATIME,
            "relatime" => flags |= MS_RELATIME,
            "strictatime" => flags |= MS_STRICTATIME,
            "lazytime" => flags |= MS_LAZYTIME,
            "bind" => flags |= MS_BIND,
            "move" => flags |= MS_MOVE,
            "silent" | "quiet" => flags |= MS_SILENT,
            "defaults" => {
                // defaults = rw,suid,dev,exec,auto,nouser,async
                flags &= !(MS_RDONLY | MS_NOSUID | MS_NODEV | MS_NOEXEC);
            }
            "" => {} // Ignore empty options
            _ => {
                // Unknown option - ignore for now
            }
        }
    }

    flags
}

fn show_help() {
    println!("Usage: mount [-t type] [-o options] source target");
    println!("       mount              List all mounts");
    println!("       mount -a           Mount all from /etc/fstab");
    println!();
    println!("Options:");
    println!("  -t type     Filesystem type (ext4, tmpfs, procfs, devfs)");
    println!("  -o options  Mount options (comma-separated):");
    println!("              ro, rw, noatime, nosuid, nodev, noexec,");
    println!("              relatime, strictatime, remount, defaults");
    println!("  -a          Mount all filesystems in /etc/fstab");
    println!("  -r          Mount read-only (same as -o ro)");
    println!("  -w          Mount read-write (same as -o rw)");
    println!("  -v          Verbose output");
    println!("  --help      Show this help message");
}

/// List currently mounted filesystems by reading /proc/mounts
fn list_mounts() {
    let fd = libc::open("/proc/mounts", libc::O_RDONLY, 0);
    if fd < 0 {
        // /proc/mounts not available, try alternative
        println!("mount: cannot read /proc/mounts");
        return;
    }

    let mut buf = [0u8; 4096];
    let n = libc::read(fd, &mut buf);
    libc::close(fd);

    if n > 0 {
        // Print the mounts data
        let data = unsafe { core::str::from_utf8_unchecked(&buf[..n as usize]) };
        print!("{}", data);
    } else {
        println!("(no mounts)");
    }
}

/// Parse and mount entries from /etc/fstab
fn mount_fstab(verbose: bool) -> i32 {
    let fd = libc::open("/etc/fstab", libc::O_RDONLY, 0);
    if fd < 0 {
        println!("mount: cannot read /etc/fstab");
        return 1;
    }

    let mut buf = [0u8; 4096];
    let n = libc::read(fd, &mut buf);
    libc::close(fd);

    if n <= 0 {
        return 0;
    }

    let data = unsafe { core::str::from_utf8_unchecked(&buf[..n as usize]) };
    let mut errors = 0;

    for line in data.lines() {
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Parse fstab entry: device mountpoint fstype options dump pass
        let mut parts = line.split_whitespace();

        let device = match parts.next() {
            Some(d) => d,
            None => continue,
        };

        let mountpoint = match parts.next() {
            Some(m) => m,
            None => continue,
        };

        let fstype = match parts.next() {
            Some(f) => f,
            None => continue,
        };

        let options = parts.next().unwrap_or("defaults");

        // Skip entries with "noauto" option
        if options.split(',').any(|o| o == "noauto") {
            continue;
        }

        // Parse options
        let flags = parse_options(options);

        if verbose {
            println!("mount: mounting {} on {} type {} ({})", device, mountpoint, fstype, options);
        }

        // Perform the mount
        let result = libc::syscall::mount(device, mountpoint, fstype, flags, ptr::null());

        if result < 0 {
            println!("mount: mounting {} on {} failed: error {}", device, mountpoint, result);
            errors += 1;
        }
    }

    errors
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    let mut args = Args::new();
    let mut positional = [None::<&str>; 2];
    let mut pos_count = 0;

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
        } else if arg == "-a" {
            args.mount_all = true;
        } else if arg == "-r" {
            args.flags |= MS_RDONLY;
        } else if arg == "-w" {
            args.flags &= !MS_RDONLY;
        } else if arg == "-v" {
            args.verbose = true;
        } else if arg == "-t" {
            // Filesystem type
            i += 1;
            if i < argc as usize {
                args.fstype = Some(unsafe {
                    let ptr = *argv.add(i);
                    let mut len = 0;
                    while *ptr.add(len) != 0 {
                        len += 1;
                    }
                    core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr, len))
                });
            }
        } else if arg == "-o" {
            // Mount options
            i += 1;
            if i < argc as usize {
                let opts = unsafe {
                    let ptr = *argv.add(i);
                    let mut len = 0;
                    while *ptr.add(len) != 0 {
                        len += 1;
                    }
                    core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr, len))
                };
                args.flags |= parse_options(opts);
            }
        } else if !arg.starts_with('-') {
            // Positional argument
            if pos_count < 2 {
                positional[pos_count] = Some(arg);
                pos_count += 1;
            }
        } else {
            println!("mount: unknown option: {}", arg);
            return 1;
        }

        i += 1;
    }

    // Handle --help
    if args.show_help {
        show_help();
        return 0;
    }

    // Handle -a (mount all)
    if args.mount_all {
        return mount_fstab(args.verbose);
    }

    // Handle no arguments (list mounts)
    if pos_count == 0 {
        list_mounts();
        return 0;
    }

    // Need at least source and target
    if pos_count < 2 {
        println!("mount: missing target");
        println!("Usage: mount [-t type] [-o options] source target");
        return 1;
    }

    let source = positional[0].unwrap();
    let target = positional[1].unwrap();

    // Determine filesystem type
    let fstype = args.fstype.unwrap_or_else(|| {
        // Try to auto-detect from source
        if source.starts_with("/dev/") {
            "ext4" // Default for block devices
        } else if source == "none" || source == "tmpfs" {
            "tmpfs"
        } else if source == "proc" {
            "proc"
        } else if source == "sysfs" || source == "sys" {
            "sysfs"
        } else if source == "devpts" {
            "devpts"
        } else {
            "auto"
        }
    });

    if args.verbose {
        println!("mount: {} -> {} type {} flags={:#x}", source, target, fstype, args.flags);
    }

    // Perform the mount
    let result = libc::syscall::mount(source, target, fstype, args.flags, ptr::null());

    if result < 0 {
        let err = -result;
        let msg = match err {
            1 => "operation not permitted",
            2 => "no such file or directory",
            5 => "I/O error",
            13 => "permission denied",
            16 => "device or resource busy",
            19 => "no such device",
            22 => "invalid argument",
            38 => "function not implemented",
            _ => "unknown error",
        };
        println!("mount: mounting {} on {} failed: {} ({})", source, target, msg, err);
        return 1;
    }

    if args.verbose {
        println!("mount: {} mounted on {}", source, target);
    }

    0
}
