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
    // — GraveShift: Don't just check /sbin/init exists. Verify the ext4 root is
    // actually usable. A half-assed pivot is worse than no pivot at all.
    let fd = open("/mnt/root/sbin/init", O_RDONLY, 0);
    if fd < 0 {
        printlns("[init] No ext4 root at /mnt/root, staying on initramfs");
        return false;
    }
    close(fd);

    // — GraveShift: Paranoia checks disabled — ext4 directory block reads hang
    // when the /bin/ directory spans multiple blocks. The VirtIO-blk second read
    // deadlocks somewhere in the block I/O path. Skip these checks and trust the
    // pivot_root will fail gracefully if files are actually missing.
    // TODO: fix the ext4/virtio-blk block read hang (BUG: second block read for
    // large directories deadlocks)

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

    // — GraveShift: Spawn getty on every active VT. Probe /dev/tty1, /dev/tty2, ...
    // until we hit ENOENT. The kernel registers only VT_COUNT devices, so this
    // auto-discovers how many VTs exist without hardcoding the count in userspace.
    printlns("[init] Spawning getty on available VTs...");
    let mut getty_pids: [i64; 6] = [0; 6]; // — GraveShift: max 6 VTs, track PIDs for respawn
    let mut num_gettys: usize = 0;

    let mut vt_num = 1u8; // /dev/tty1 through /dev/ttyN
    while vt_num <= 6 {
        // Build "/dev/ttyN" path
        let tty_path: [u8; 10] = [
            b'/', b'd', b'e', b'v', b'/', b't', b't', b'y', b'0' + vt_num, 0,
        ];
        let tty_path_str = unsafe {
            core::str::from_utf8_unchecked(&tty_path[..9])
        };

        // Probe — if the device doesn't exist, we're done
        let probe_fd = open(tty_path_str, O_RDONLY, 0);
        if probe_fd < 0 {
            break;
        }
        close(probe_fd);

        let child = fork();
        if child == 0 {
            // — GraveShift: child — become session leader, open /dev/ttyN as
            // stdin/stdout/stderr, then exec getty. This gives each VT its own
            // controlling terminal. Login on VT2 is independent of VT1.
            setsid();
            close(0);
            close(1);
            close(2);
            let fd = open(tty_path_str, O_RDWR, 0); // fd 0 (stdin)
            if fd < 0 {
                _exit(1);
            }
            dup2(fd, 1); // stdout
            dup2(fd, 2); // stderr
            exec("/bin/getty");
            _exit(1);
        } else if child > 0 {
            prints("[init] Getty on ");
            prints(tty_path_str);
            prints(" (pid ");
            print_i64(child as i64);
            printlns(")");
            if (num_gettys) < 6 {
                getty_pids[num_gettys] = child as i64;
                num_gettys += 1;
            }
        } else {
            prints("[init] Fork failed for ");
            printlns(tty_path_str);
        }

        vt_num += 1;
    }

    if num_gettys == 0 {
        eprintlns("[init] No VT devices found! Falling back to /dev/console getty");
        let child = fork();
        if child == 0 {
            exec("/bin/getty");
            _exit(1);
        } else if child > 0 {
            getty_pids[0] = child as i64;
            num_gettys = 1;
        }
    }

    // — GraveShift: Screen dump gated behind debug-screendump feature.
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

    // — GraveShift: reap zombies forever, respawning gettys when they exit
    reap_zombies_multi(&mut getty_pids, num_gettys);

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
            // Child - exec servicemgr in explicit daemon mode
            setsid();
            let arg0 = b"servicemgr\0";
            let arg1 = b"daemon\0";
            let argv = [arg0.as_ptr(), arg1.as_ptr(), core::ptr::null()];
            execv("/bin/servicemgr", argv.as_ptr());
            _exit(1);
        } else if child > 0 {
            printlns("[init] Service manager started");
        }
    } else {
        eprintlns("[init] /bin/servicemgr not found, skipping");
    }
}

/// Reap zombie processes forever, respawning gettys when they exit.
/// — GraveShift: tracks PIDs for all VT gettys. When one exits, respawn
/// it on the same /dev/ttyN with a fresh setsid + fd setup.
fn reap_zombies_multi(getty_pids: &mut [i64; 6], num_gettys: usize) -> ! {
    loop {
        let mut status: i32 = 0;
        let pid = wait(&mut status);

        if pid > 0 {
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

            // — GraveShift: check if this was one of our gettys. If so, respawn
            // on the same VT after a brief cooldown to avoid respawn storms.
            let mut respawn_vt: Option<u8> = None;
            for i in 0..num_gettys {
                if getty_pids[i] == pid as i64 {
                    respawn_vt = Some((i + 1) as u8); // VT numbers are 1-indexed
                    getty_pids[i] = 0;
                    break;
                }
            }

            if let Some(vt) = respawn_vt {
                sleep(2);
                let tty_path: [u8; 10] = [
                    b'/', b'd', b'e', b'v', b'/', b't', b't', b'y', b'0' + vt, 0,
                ];
                let tty_path_str = unsafe {
                    core::str::from_utf8_unchecked(&tty_path[..9])
                };

                prints("[init] Respawning getty on ");
                printlns(tty_path_str);

                let child = fork();
                if child == 0 {
                    setsid();
                    close(0);
                    close(1);
                    close(2);
                    let fd = open(tty_path_str, O_RDWR, 0);
                    if fd < 0 { _exit(1); }
                    dup2(fd, 1);
                    dup2(fd, 2);
                    exec("/bin/getty");
                    _exit(1);
                } else if child > 0 {
                    getty_pids[(vt - 1) as usize] = child as i64;
                }
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
