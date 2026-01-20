//! OXIDE Init Process (PID 1)
//!
//! First userspace process that:
//! - Mounts essential filesystems
//! - Starts getty/login
//! - Reaps orphaned zombie processes
//! - Handles shutdown

#![no_std]
#![no_main]

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

    // Run fbtest to test framebuffer
    printlns("[init] Running framebuffer test...");
    let fb_child = fork();
    if fb_child == 0 {
        let ret = exec("/initramfs/bin/fbtest");
        if ret < 0 {
            eprintlns("[init] Failed to exec fbtest");
        }
        _exit(0);
    } else if fb_child > 0 {
        // Wait for fbtest to complete
        let mut status: i32 = 0;
        wait(&mut status);
        printlns("[init] Framebuffer test completed");
    }

    // Spawn a shell directly for now (no getty/login)
    printlns("[init] Spawning shell...");

    let child = fork();
    if child == 0 {
        // Child process - exec shell
        // Try initramfs paths first, then /bin paths
        let paths = ["/initramfs/bin/esh", "/initramfs/bin/sh", "/bin/esh", "/bin/sh"];
        for path in paths.iter() {
            let ret = exec(path);
            if ret >= 0 {
                // exec succeeded, should not return
                break;
            }
        }
        eprintlns("[init] Failed to exec shell");
        _exit(1);
    } else if child > 0 {
        // Parent - reap zombies forever
        printlns("[init] Shell started");
        reap_zombies();
    } else {
        eprintlns("[init] Fork failed");
    }

    // Should never reach here
    0
}

/// Reap zombie processes forever
fn reap_zombies() -> ! {
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

            // If shell died, spawn a new one
            printlns("[init] Respawning shell...");
            let child = fork();
            if child == 0 {
                let paths = ["/initramfs/bin/esh", "/initramfs/bin/sh", "/bin/esh", "/bin/sh"];
                for path in paths.iter() {
                    let _ = exec(path);
                }
                eprintlns("[init] Failed to exec shell");
                _exit(1);
            }
        }
    }
}
