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

/// Main init entry point
#[unsafe(no_mangle)]
fn main() -> i32 {
    printlns("OXIDE init starting...");

    // We're PID 1
    let pid = getpid();
    if pid != 1 {
        eprintlns("Warning: init is not PID 1!");
    }

    // Print startup message
    printlns("");
    printlns("OXIDE OS v0.1.0");
    printlns("");

    // Mount filesystems from /etc/fstab
    mount_fstab();

    // Load firewall rules early in boot
    load_firewall_rules();

    // Start service manager in daemon mode
    start_servicemgr();

    // Run loopback ping test
    printlns("[init] Testing loopback ping...");
    let test_child = fork();
    if test_child == 0 {
        // Child - run ping test
        // Build argv array with null terminator
        let arg0 = b"/bin/ping\0".as_ptr();
        let arg1 = b"-c\0".as_ptr();
        let arg2 = b"1\0".as_ptr();
        let arg3 = b"127.0.0.1\0".as_ptr();
        let argv: [*const u8; 5] = [arg0, arg1, arg2, arg3, core::ptr::null()];
        execv("/bin/ping", argv.as_ptr());
        eprintlns("[init] Failed to exec ping");
        _exit(1);
    } else if test_child > 0 {
        // Wait for ping test to complete
        let mut status: i32 = 0;
        waitpid(test_child, &mut status, 0);
        if status == 0 {
            printlns("[init] Loopback ping test PASSED");
        } else {
            prints("[init] Loopback ping test FAILED (status=");
            print_i64(status as i64);
            printlns(")");
        }
    }

    // Test Python execution - minimal mode
    printlns("[init] Testing Python...");
    let python_child = fork();
    if python_child == 0 {
        // Child - run Python in completely isolated mode
        let arg0 = b"/usr/bin/python3\0".as_ptr();
        let arg1 = b"-I\0".as_ptr();  // Isolated mode (implies -E -s)
        let arg2 = b"-S\0".as_ptr();  // Don't add site to sys.path
        let arg3 = b"-c\0".as_ptr();
        let arg4 = b"import sys; print('Python', sys.version_info[:2])\0".as_ptr();
        let argv: [*const u8; 6] = [arg0, arg1, arg2, arg3, arg4, core::ptr::null()];
        execv("/usr/bin/python3", argv.as_ptr());
        eprintlns("[init] Failed to exec python");
        _exit(1);
    } else if python_child > 0 {
        // Wait for Python test to complete
        let mut status: i32 = 0;
        waitpid(python_child, &mut status, 0);
        if status == 0 {
            printlns("[init] Python test PASSED!");
        } else {
            prints("[init] Python test FAILED (status=");
            print_i64(status as i64);
            printlns(")");
        }
    }

    // Spawn getty/login on the primary TTY
    printlns("[init] Spawning getty/login...");
    let child = fork();
    if child == 0 {
        // Child process - exec getty which launches login
        exec("/bin/getty");
        // Fallback to login directly
        exec("/bin/login");
        printlns("[init/child] Login failed, trying shell...");
        exec("/bin/esh");
        eprintlns("[init] Failed to exec getty/login/shell");
        _exit(1);
    } else if child > 0 {
        // Parent - reap zombies forever, respawning getty when it exits
        printlns("[init] Getty started (PID ");
        print_i64(child as i64);
        printlns(")");
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

    let mut buf = [0u8; 2048];
    let n = read(fd, &mut buf);
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

        // Parse options into flags
        let flags = parse_mount_options(options);

        // Perform the mount
        prints("[init]   Mounting ");
        prints(device);
        prints(" on ");
        prints(mountpoint);
        prints("...");

        let result = syscall::mount(device, mountpoint, fstype, flags, core::ptr::null());

        if result == 0 {
            printlns(" OK");
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

/// Start service manager in daemon mode
fn start_servicemgr() {
    // Check if servicemgr exists
    let fd = open("/bin/servicemgr", 0, 0);
    if fd >= 0 {
        close(fd);
        printlns("[init] Starting service manager...");

        let child = fork();
        if child == 0 {
            // Child - exec servicemgr daemon
            // Create new session so servicemgr runs independently
            setsid();
            let argv: [*const u8; 3] = [
                b"/bin/servicemgr\0".as_ptr(),
                b"daemon\0".as_ptr(),
                core::ptr::null(),
            ];
            execv("/bin/servicemgr", argv.as_ptr());
            _exit(1);
        } else if child > 0 {
            printlns("[init] Service manager started");
        }
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
                printlns("[init] Getty exited, respawning...");
                let child = fork();
                if child == 0 {
                    let _ = exec("/bin/getty");
                    let _ = exec("/bin/login");
                    let _ = exec("/bin/esh");
                    eprintlns("[init] Failed to exec getty/login/shell");
                    _exit(1);
                } else if child > 0 {
                    getty_pid = child as i64; // Update tracked getty PID
                    prints("[init] New getty started (PID ");
                    print_i64(getty_pid);
                    printlns(")");
                }
            } else {
                // Some other descendant process exited (login, shell, etc)
                // Don't respawn - getty will handle it
                prints("[init] Descendant process exited, getty still running (PID ");
                print_i64(getty_pid);
                printlns(")");
            }
        }
    }
}
