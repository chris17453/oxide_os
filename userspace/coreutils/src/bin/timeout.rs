//! timeout - run a command with a time limit

#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 3 {
        eprintlns("usage: timeout <duration> <command> [args...]");
        return 1;
    }

    let duration_str = unsafe { cstr_to_str(*argv.add(1)) };

    // Parse duration (in seconds)
    let duration = match parse_int(duration_str.as_bytes()) {
        Some(v) => v,
        None => {
            eprintlns("timeout: invalid duration");
            return 1;
        }
    };

    if duration <= 0 {
        eprintlns("timeout: duration must be positive");
        return 1;
    }

    prints("timeout: would run command with ");
    print_i64(duration);
    prints("s timeout: ");

    for i in 2..argc {
        let arg = unsafe { cstr_to_str(*argv.add(i as usize)) };
        prints(arg);
        if i < argc - 1 {
            prints(" ");
        }
    }
    printlns("");

    // In a full implementation, we would:
    // 1. Fork a child process
    // 2. In parent: set up an alarm/timer for the duration
    // 3. In child: execute the command
    // 4. In parent: wait for child or timeout
    // 5. If timeout occurs, send SIGTERM to child
    // 6. If child doesn't exit after grace period, send SIGKILL

    // This requires:
    // - fork() - available
    // - execv() - available
    // - alarm() or timer_create() - not yet implemented
    // - waitpid() with timeout - available
    // - kill() - available

    eprintlns("timeout: timer syscalls not yet implemented");

    1
}

fn parse_int(s: &[u8]) -> Option<i64> {
    if s.is_empty() {
        return None;
    }

    let mut result: i64 = 0;
    for &c in s {
        if c < b'0' || c > b'9' {
            return None;
        }
        result = result * 10 + (c - b'0') as i64;
    }

    Some(result)
}

fn cstr_to_str(ptr: *const u8) -> &'static str {
    if ptr.is_null() {
        return "";
    }
    let mut len = 0;
    unsafe {
        while *ptr.add(len) != 0 {
            len += 1;
        }
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr, len))
    }
}
