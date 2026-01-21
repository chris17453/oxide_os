//! nohup - run a command immune to hangups

#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        eprintlns("usage: nohup <command> [args...]");
        return 1;
    }

    // In a full implementation, we would:
    // 1. Ignore SIGHUP signal
    // 2. Redirect stdout/stderr to nohup.out if they're terminals
    // 3. Execute the command
    
    prints("nohup: would run: ");
    for i in 1..argc {
        let arg = unsafe { cstr_to_str(*argv.add(i as usize)) };
        prints(arg);
        if i < argc - 1 {
            prints(" ");
        }
    }
    printlns("");

    // Set up signal handling to ignore SIGHUP
    // This would require sigaction syscall:
    // let sa = SigAction {
    //     sa_handler: SIG_IGN,
    //     ...
    // };
    // sigaction(SIGHUP, &sa, null_mut());

    // Redirect output to nohup.out if stdout is a terminal
    // This would check if stdout is a tty and open nohup.out

    // Execute the command
    // This would use execv/execve

    eprintlns("nohup: signal handling not yet fully implemented");
    
    1
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
