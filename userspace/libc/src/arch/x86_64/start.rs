//! x86_64 userspace entry point
//!
//! Entry point for x86_64 userspace programs.
//! Stack layout at entry (set up by exec):
//!   [rsp+0]  = argc
//!   [rsp+8]  = argv[0]
//!   [rsp+16] = argv[1]
//!   ...
//!   [rsp+8*(argc+1)] = NULL
//!   [rsp+8*(argc+2)] = envp[0]
//!   ...
//!
//! — GraveShift: Fixed _start to use proper stack frames instead of naked_asm

/// Entry point for x86_64 userspace programs
///
/// Reads argc/argv from stack per x86_64 ELF ABI, initializes environment,
/// calls main(), and exits with the return code.
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    unsafe {
        let argc: i32;
        let argv: *const *const u8;

        // Read argc and argv from stack per x86_64 ELF ABI
        // At entry: [rsp+0] = argc, [rsp+8] = argv[0], ...
        core::arch::asm!(
            "mov {argc:e}, [rsp]",      // Read 32-bit argc
            "lea {argv}, [rsp + 8]",    // Calculate argv pointer
            argc = out(reg) argc,
            argv = out(reg) argv,
        );

        // Initialize environment
        crate::env::init_env();

        // Initialize FILE streams (stdin/stdout/stderr)
        crate::filestream::init_stdio();

        // Initialize environ pointer for C compatibility
        crate::c_exports::init_environ();

        // Call main
        let ret = _main_wrapper(argc, argv);

        // Debug: About to exit
        let _ = crate::syscall::sys_write(2, b"[START_DEBUG] _start calling sys_exit\n");

        // Exit using the normal syscall wrapper
        crate::syscall::sys_exit(ret);
    }
}

// Wrapper to call the user's main function with argc/argv
#[inline(never)]
fn _main_wrapper(argc: i32, argv: *const *const u8) -> i32 {
    unsafe extern "Rust" {
        fn main(argc: i32, argv: *const *const u8) -> i32;
    }
    let ret = unsafe { main(argc, argv) };
    let _ = crate::syscall::sys_write(2, b"[LIBC_DEBUG] main returned with code=");
    let mut buf = [0u8; 20];
    let mut i = 19;
    let mut n = if ret < 0 {
        let _ = crate::syscall::sys_write(2, b"-");
        (-ret) as u32
    } else {
        ret as u32
    };
    if n == 0 {
        buf[19] = b'0';
        i = 19;
    } else {
        while n > 0 && i > 0 {
            i -= 1;
            buf[i] = b'0' + (n % 10) as u8;
            n /= 10;
        }
    }
    let _ = crate::syscall::sys_write(2, &buf[i..]);
    let _ = crate::syscall::sys_write(2, b"\n");
    ret
}
