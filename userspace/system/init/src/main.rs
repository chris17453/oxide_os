//! OXIDE Init Process (PID 1)
//!
//! First userspace process that:
//! - Mounts filesystems from /etc/fstab
//! - Loads firewall rules
//! - Starts service manager
//! - Starts getty/login
//! - Reaps orphaned zombie processes
//! - Handles shutdown

#![no_std]
#![no_main]

use libc::syscall::mount_flags::*;
use libc::*;

/// Known label-to-device mappings for the default disk layout
fn map_label(label: &str) -> Option<&'static str> {
    match label {
        "BOOT" => Some("/dev/virtio0p1"),
        "HOME" => Some("/dev/virtio0p3"),
        "ROOT" => Some("/dev/virtio0p2"),
        _ => None,
    }
}

/// Attempt to switch from initramfs to ext4 root filesystem.
///
/// If an ext4 root is mounted at /mnt/root (by the kernel), this function:
/// 1. Moves /dev/pts and /dev to the new root
/// 2. Calls pivot_root to swap root filesystems
/// 3. Cleans up stale initramfs mounts
///
/// Returns true if the switch succeeded, false if staying on initramfs.
fn switch_root() -> bool {
    // Check if ext4 root is available by probing for /sbin/init on it
    let fd = open("/mnt/root/sbin/init", O_RDONLY, 0);
    if fd < 0 {
        printlns("[init] No ext4 root filesystem at /mnt/root, staying on initramfs");
        return false;
    }
    close(fd);

    printlns("[init] Switching to ext4 root filesystem...");

    // Move devfs mounts to new root (children first)
    let r = syscall::mount_move("/dev/pts", "/mnt/root/dev/pts");
    if r != 0 {
        prints("[init] Warning: move /dev/pts failed (");
        print_i64(r as i64);
        printlns(")");
    }

    let r = syscall::mount_move("/dev", "/mnt/root/dev");
    if r != 0 {
        prints("[init] Warning: move /dev failed (");
        print_i64(r as i64);
        printlns("), aborting switch_root");
        // Try to move /dev/pts back if we moved it
        let _ = syscall::mount_move("/mnt/root/dev/pts", "/dev/pts");
        return false;
    }

    // pivot_root: make ext4 the new /, old initramfs at /initramfs
    let r = syscall::pivot_root("/mnt/root", "/mnt/root/initramfs");
    if r != 0 {
        prints("[init] pivot_root failed (");
        print_i64(r as i64);
        printlns("), staying on initramfs");
        // Move devfs back
        let _ = syscall::mount_move("/mnt/root/dev", "/dev");
        let _ = syscall::mount_move("/mnt/root/dev/pts", "/dev/pts");
        return false;
    }

    // Update working directory to new root
    chdir("/");

    printlns("[init] Root switched to ext4");

    // Clean up old initramfs mounts (these are now under /initramfs)
    let _ = syscall::umount("/initramfs/var/run");
    let _ = syscall::umount("/initramfs/var/lib");
    let _ = syscall::umount("/initramfs/var/log");
    let _ = syscall::umount("/initramfs/tmp");
    let _ = syscall::umount("/initramfs/run");
    let _ = syscall::umount("/initramfs/proc");
    let _ = syscall::umount("/initramfs/sys");
    let _ = syscall::umount("/initramfs");

    true
}

/// Main init entry point
#[unsafe(no_mangle)]
fn main() -> i32 {
    printlns("[init] OXIDE init starting...");

    // We're PID 1
    let pid = getpid();
    if pid != 1 {
        eprintlns("Warning: init is not PID 1!");
    }

    // Print startup message
    printlns("");
    printlns("OXIDE OS v0.1.0");
    printlns("");

    // Try to switch from initramfs to ext4 root filesystem.
    // If successful, / is now ext4 and /etc/fstab comes from there.
    // If it fails, we stay on initramfs (current behavior).
    let _switched = switch_root();

    // Mount filesystems from /etc/fstab
    // (reads ext4 fstab after switch, or initramfs fstab if no switch)
    mount_fstab();

    printlns("[init] DEBUG: mount_fstab completed, about to load firewall rules");

    // Load firewall rules early in boot
    load_firewall_rules();

    printlns("[init] DEBUG: load_firewall_rules completed");

    // Load console keyboard layout from /etc/vconsole.conf
    load_vconsole_config();

    // Start service manager in daemon mode
    start_servicemgr();

    // Spawn getty on the primary TTY
    printlns("[init] Spawning getty...");
    let child = fork();
    if child == 0 {
        // Child process - exec getty
        exec("/bin/getty");
        eprintlns("[init] Failed to exec getty");
        _exit(1);
    } else if child > 0 {
        // Parent - reap zombies forever, respawning getty when it exits
        printlns("[init] Getty started");

        // — GraveShift: Screen dump gated behind debug-screendump feature.
        // Holds TERMINAL mutex for ~1.3s during 14KB serial output, blocking
        // the shell from printing its prompt. Only enable for VT debugging.
        #[cfg(feature = "debug-screendump")]
        {
            printlns("[init] DEBUG: Waiting 3 seconds before screen dump...");
            sleep(3);
            printlns("[init] DEBUG: Calling syscall 999 to dump screen to serial...");
            let result: i64;
            unsafe {
                core::arch::asm!(
                    "mov rax, 999",
                    "syscall",
                    out("rax") result,
                    lateout("rcx") _,
                    lateout("r11") _,
                );
            }
            prints("[init] DEBUG: Syscall 999 returned: ");
            print_i64(result);
            printlns("");
        }

        reap_zombies(child as i64);
    } else {
        eprintlns("[init] Fork failed");
    }

    // Should never reach here
    0
}

/// Mount filesystems from /etc/fstab
fn mount_fstab() {
    let fd = open("/etc/fstab", O_RDONLY, 0);
    if fd < 0 {
        printlns("[init] No /etc/fstab found");
        return;
    }

    printlns("[init] Mounting filesystems from /etc/fstab...");
    printlns("[init] DEBUG: A - after first printlns");
    printlns("[init] DEBUG: A2 - about to allocate buf");

    let mut buf = [0u8; 2048];
    printlns("[init] DEBUG: B - after buf allocation");
    printlns("[init] DEBUG: About to read fstab");
    let n = read(fd, &mut buf);
    printlns("[init] DEBUG: Read fstab completed");
    close(fd);

    if n <= 0 {
        return;
    }

    // Parse fstab line by line
    // Format: device mountpoint fstype options dump pass
    let content = unsafe { core::str::from_utf8_unchecked(&buf[..n as usize]) };

    for line in content.lines() {
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Parse fields
        let mut fields = line.split_whitespace();

        let device = match fields.next() {
            Some(d) => d,
            None => continue,
        };

        let mountpoint = match fields.next() {
            Some(m) => m,
            None => continue,
        };

        let fstype = match fields.next() {
            Some(f) => f,
            None => continue,
        };

        let options = fields.next().unwrap_or("defaults");

        // Skip "noauto" entries
        if options.split(',').any(|o| o == "noauto") {
            continue;
        }

        // Resolve LABEL=foo entries to concrete device nodes
        let mut device_path_buf: [u8; 64] = [0; 64];
        let device_resolved = if let Some(stripped) = device.strip_prefix("LABEL=") {
            if let Some(mapped) = map_label(stripped) {
                mapped
            } else {
                // Try /dev/disk/by-label/<label> fallback
                let suffix = stripped.as_bytes();
                let prefix = b"/dev/disk/by-label/";
                if prefix.len() + suffix.len() < device_path_buf.len() {
                    device_path_buf[..prefix.len()].copy_from_slice(prefix);
                    device_path_buf[prefix.len()..prefix.len() + suffix.len()]
                        .copy_from_slice(suffix);
                    unsafe {
                        core::str::from_utf8_unchecked(
                            &device_path_buf[..prefix.len() + suffix.len()],
                        )
                    }
                } else {
                    device
                }
            }
        } else {
            device
        };

        // Parse options into flags
        let flags = parse_mount_options(options);
        let is_ro = flags & MS_RDONLY != 0;

        // Perform the mount
        prints("[init]   Mounting ");
        prints(device_resolved);
        prints(" on ");
        prints(mountpoint);
        prints(" (fs=");
        prints(fstype);
        prints(", flags=");
        if is_ro {
            prints("ro");
        } else {
            prints("rw");
        }
        prints(")...");

        let result = syscall::mount(
            device_resolved,
            mountpoint,
            fstype,
            flags,
            core::ptr::null(),
        );

        if result == 0 {
            printlns(" OK");
        } else if result == -16 {
            printlns(" already mounted");
        } else {
            prints(" FAILED (");
            print_i64(result as i64);
            printlns(")");
        }
    }
}

/// Parse mount options string into flags
fn parse_mount_options(opts: &str) -> u32 {
    let mut flags = 0u32;

    for opt in opts.split(',') {
        let opt = opt.trim();
        match opt {
            "ro" | "rdonly" => flags |= MS_RDONLY,
            "rw" => flags &= !MS_RDONLY,
            "nosuid" => flags |= MS_NOSUID,
            "nodev" => flags |= MS_NODEV,
            "noexec" => flags |= MS_NOEXEC,
            "sync" => flags |= MS_SYNCHRONOUS,
            "noatime" => flags |= MS_NOATIME,
            "nodiratime" => flags |= MS_NODIRATIME,
            "relatime" => flags |= MS_RELATIME,
            "defaults" => {
                // defaults = rw,suid,dev,exec,auto,nouser,async
                flags &= !(MS_RDONLY | MS_NOSUID | MS_NODEV | MS_NOEXEC);
            }
            _ => {} // Ignore unknown options
        }
    }

    flags
}

/// Load firewall rules from /etc/fw.rules if it exists
fn load_firewall_rules() {
    // Check if rules file exists by trying to open it
    let fd = open("/etc/fw.rules", 0, 0);
    if fd >= 0 {
        close(fd);
        printlns("[init] Loading firewall rules...");

        let child = fork();
        if child == 0 {
            // Child - exec fw restore
            let argv: [*const u8; 4] = [
                b"/bin/fw\0".as_ptr(),
                b"restore\0".as_ptr(),
                b"/etc/fw.rules\0".as_ptr(),
                core::ptr::null(),
            ];
            execv("/bin/fw", argv.as_ptr());
            _exit(1);
        } else if child > 0 {
            // Wait for fw to complete
            let mut status: i32 = 0;
            waitpid(child, &mut status, 0);
            if wifexited(status) && wexitstatus(status) == 0 {
                printlns("[init] Firewall rules loaded");
            } else {
                eprintlns("[init] Failed to load firewall rules");
            }
        }
    } else {
        printlns("[init] No firewall rules file found");
    }
}

/// Load console configuration from /etc/vconsole.conf
/// — GraveShift: Linux-standard config file. Reads KEYMAP= and sets keyboard layout
/// via SYS_SETKEYMAP syscall. If the file doesn't exist, defaults stay (US QWERTY).
fn load_vconsole_config() {
    let fd = open("/etc/vconsole.conf", 0, 0);
    if fd < 0 {
        return;
    }

    let mut buf = [0u8; 512];
    let n = read(fd, &mut buf);
    close(fd);
    if n <= 0 {
        return;
    }

    let content = unsafe { core::str::from_utf8_unchecked(&buf[..n as usize]) };

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }

        if let Some(keymap) = line.strip_prefix("KEYMAP=") {
            let keymap = keymap.trim();
            if !keymap.is_empty() {
                let result = libc::syscall::syscall2(
                    libc::syscall::SYS_SETKEYMAP,
                    keymap.as_ptr() as usize,
                    keymap.len(),
                );
                if result == 0 {
                    printlns("[init] Keyboard layout set from /etc/vconsole.conf");
                } else {
                    eprintlns("[init] Failed to set keyboard layout");
                }
            }
        }
    }
}

/// Start service manager in daemon mode
fn start_servicemgr() {
    // Check if servicemgr exists
    let fd = open("/bin/servicemgr", 0, 0);
    if fd >= 0 {
        close(fd);
        printlns("[init] Starting service manager...");

        let child = fork();
        if child == 0 {
            // Child - exec servicemgr (defaults to daemon mode when argc < 2)
            setsid();
            exec("/bin/servicemgr");
            _exit(1);
        } else if child > 0 {
            printlns("[init] Service manager started");
        }
    } else {
        eprintlns("[init] /bin/servicemgr not found, skipping");
    }
}

/// Reap zombie processes forever, respawning getty when it exits
fn reap_zombies(mut getty_pid: i64) -> ! {
    loop {
        let mut status: i32 = 0;
        let pid = wait(&mut status);

        if pid > 0 {
            // Child exited
            prints("[init] Reaped process ");
            print_i64(pid as i64);

            if wifexited(status) {
                prints(" (exit status ");
                print_i64(wexitstatus(status) as i64);
                printlns(")");
            } else if wifsignaled(status) {
                prints(" (killed by signal ");
                print_i64(wtermsig(status) as i64);
                printlns(")");
            } else {
                printlns("");
            }

            // Only respawn getty if getty itself (our direct child) exited
            if pid as i64 == getty_pid {
                printlns("[init] Getty exited, waiting before respawn...");
                // Sleep for 2 seconds to avoid rapid respawn loop
                sleep(2);
                printlns("[init] Respawning getty...");
                let child = fork();
                if child == 0 {
                    let _ = exec("/bin/getty");
                    eprintlns("[init] Failed to exec getty");
                    _exit(1);
                } else if child > 0 {
                    getty_pid = child as i64; // Update tracked getty PID
                    printlns("[init] New getty started");
                }
            } else {
                // Some other descendant process exited
                printlns("[init] Descendant process exited, getty still running");
            }
        }
    }
}

/// Sleep for specified seconds
fn sleep(seconds: u64) {
    let ts = TimeSpec {
        tv_sec: seconds as i64,
        tv_nsec: 0,
    };
    let mut rem = TimeSpec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    unsafe {
        nanosleep(&ts as *const TimeSpec, &mut rem as *mut TimeSpec);
    }
}

#[repr(C)]
struct TimeSpec {
    tv_sec: i64,
    tv_nsec: i64,
}

unsafe extern "C" {
    fn nanosleep(req: *const TimeSpec, rem: *mut TimeSpec) -> i32;
}
