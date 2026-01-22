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

    // Print startup message (no framebuffer self-test)
    printlns("");
    printlns("OXIDE OS v0.1.0");
    printlns("[init] Framebuffer test disabled; booting directly");
    printlns("");

    // Spawn getty/login on the primary TTY
    printlns("[init] Spawning getty/login...");
    let child = fork();
    if child == 0 {
        // Child process - exec getty which launches login
        exec("/bin/getty");
        // Fallback to login directly
        exec("/bin/login");
        // Last resort: shell
        exec("/bin/esh");
        eprintlns("[init] Failed to exec getty/login/shell");
        _exit(1);
    } else if child > 0 {
        // Parent - reap zombies forever
        printlns("[init] Getty started");
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

            // If session died, respawn getty/login
            printlns("[init] Respawning getty...");
            let child = fork();
            if child == 0 {
                let _ = exec("/bin/getty");
                let _ = exec("/bin/login");
                let _ = exec("/bin/esh");
                eprintlns("[init] Failed to exec getty/login/shell");
                _exit(1);
            }
        }
    }
}
