//! kill - send a signal to a process

#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        eprintlns("Usage: kill [-SIGNAL] PID...");
        return 1;
    }

    let mut signal = 15; // SIGTERM is default
    let mut start_idx = 1;

    // Check if first argument is a signal number
    let first_arg = unsafe { *argv.offset(1) };
    if unsafe { *first_arg } == b'-' {
        // Parse signal number
        signal = 0;
        let mut offset = 1;
        loop {
            let ch = unsafe { *first_arg.offset(offset) };
            if ch == 0 {
                break;
            }
            if ch >= b'0' && ch <= b'9' {
                signal = signal * 10 + (ch - b'0') as i32;
            } else {
                eprintlns("kill: invalid signal number");
                return 1;
            }
            offset += 1;
        }
        start_idx = 2;

        if start_idx >= argc {
            eprintlns("kill: no PID specified");
            return 1;
        }
    }

    let mut success = true;

    // Send signal to each PID
    for i in start_idx..argc {
        let arg_ptr = unsafe { *argv.offset(i as isize) };

        // Parse PID
        let mut pid: i32 = 0;
        let mut offset = 0;
        loop {
            let ch = unsafe { *arg_ptr.offset(offset) };
            if ch == 0 {
                break;
            }
            if ch >= b'0' && ch <= b'9' {
                pid = pid * 10 + (ch - b'0') as i32;
            } else {
                eprints("kill: invalid PID '");
                // Print the argument
                let mut j = 0;
                loop {
                    let c = unsafe { *arg_ptr.offset(j) };
                    if c == 0 {
                        break;
                    }
                    eputchar(c as i32);
                    j += 1;
                }
                eprintlns("'");
                success = false;
                break;
            }
            offset += 1;
        }

        if offset > 0 && success {
            // Send the signal
            let result = kill(pid, signal);
            if result < 0 {
                eprints("kill: cannot kill process ");
                print_i64(pid as i64);
                eprintlns("");
                success = false;
            }
        }
    }

    if success { 0 } else { 1 }
}
