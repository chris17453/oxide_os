//! Getty - terminal manager for EFFLUX OS
//!
//! Opens a terminal device, configures it, and spawns login.
//! Typically started by init for each configured terminal.

#![no_std]
#![no_main]

use libc::*;

/// Terminal configuration
struct TermConfig {
    /// Terminal device path
    device: &'static str,
    /// Baud rate (for serial)
    baud: u32,
    /// Terminal type (for TERM env var)
    term_type: &'static str,
}

impl Default for TermConfig {
    fn default() -> Self {
        TermConfig {
            device: "/dev/tty1",
            baud: 115200,
            term_type: "vt100",
        }
    }
}

/// Clear screen escape sequence
fn clear_screen() {
    prints("\x1b[2J\x1b[H");
}

/// Print system identification banner
fn print_banner() {
    prints("\n");
    prints("EFFLUX OS v0.1.0\n");
    prints("================\n");
    prints("\n");
}

/// Open and configure terminal device
fn setup_terminal(config: &TermConfig) -> i32 {
    // Close any existing stdio
    close(0);
    close(1);
    close(2);

    // Open terminal device as stdin
    let fd = open2(config.device, O_RDWR);
    if fd < 0 {
        return -1;
    }

    // Duplicate for stdout and stderr
    dup2(fd, 0);
    dup2(fd, 1);
    dup2(fd, 2);

    // If opened fd was > 2, close it
    if fd > 2 {
        close(fd);
    }

    // Make this the controlling terminal (setsid + ioctl TIOCSCTTY)
    // For now, we skip this as it requires more kernel support

    0
}

/// Parse command line arguments
fn parse_args() -> TermConfig {
    // In a real implementation, we'd parse argv for:
    // - Terminal device path
    // - Baud rate
    // - Options like -L (local line), -h (hardware flow control)
    //
    // For now, return defaults
    TermConfig::default()
}

/// Main entry point
#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let config = parse_args();

    loop {
        // Setup terminal
        if setup_terminal(&config) < 0 {
            // Can't print error since stdio not set up
            exit(1);
        }

        // Clear screen and print banner
        clear_screen();
        print_banner();

        // Print terminal info
        prints("Terminal: ");
        prints(config.device);
        prints("\n\n");

        // Fork and exec login
        let pid = fork();
        if pid < 0 {
            prints("getty: fork failed\n");
            exit(1);
        }

        if pid == 0 {
            // Child - exec login
            exec("/bin/login");
            // If exec fails, try shell directly
            exec("/bin/esh");
            prints("getty: failed to exec login\n");
            exit(1);
        }

        // Parent - wait for login to exit
        let mut status = 0;
        waitpid(pid, &mut status, 0);

        // Login exited - respawn after brief delay
        // In real system, might check for rapid respawn and slow down
    }
}
