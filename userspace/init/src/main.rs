//! EFFLUX Init Process (PID 1)
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
    println("EFFLUX init starting...");

    // We're PID 1
    let pid = getpid();
    if pid != 1 {
        eprintln("Warning: init is not PID 1!");
    }

    // Print startup message
    println("");
    println("EFFLUX OS v0.1.0");
    println("");

    // Run fbtest to test framebuffer
    println("[init] Running framebuffer test...");
    let fb_child = fork();
    if fb_child == 0 {
        let ret = exec("/initramfs/bin/fbtest");
        if ret < 0 {
            eprintln("[init] Failed to exec fbtest");
        }
        _exit(0);
    } else if fb_child > 0 {
        // Wait for fbtest to complete
        let mut status: i32 = 0;
        wait(&mut status);
        println("[init] Framebuffer test completed");
    }

    // Spawn a shell directly for now (no getty/login)
    println("[init] Spawning shell...");

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
        eprintln("[init] Failed to exec shell");
        _exit(1);
    } else if child > 0 {
        // Parent - reap zombies forever
        println("[init] Shell started");
        reap_zombies();
    } else {
        eprintln("[init] Fork failed");
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
            print("[init] Reaped process ");
            print_i64(pid as i64);

            if wifexited(status) {
                print(" (exit status ");
                print_i64(wexitstatus(status) as i64);
                println(")");
            } else if wifsignaled(status) {
                print(" (killed by signal ");
                print_i64(wtermsig(status) as i64);
                println(")");
            } else {
                println("");
            }

            // If shell died, spawn a new one
            println("[init] Respawning shell...");
            let child = fork();
            if child == 0 {
                let paths = ["/initramfs/bin/esh", "/initramfs/bin/sh", "/bin/esh", "/bin/sh"];
                for path in paths.iter() {
                    let _ = exec(path);
                }
                eprintln("[init] Failed to exec shell");
                _exit(1);
            }
        }
    }
}
