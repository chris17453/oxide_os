//! ARM64 (aarch64) userspace entry point
//!
//! Entry point for ARM64 userspace programs.
//! Stack layout at entry (set up by exec):
//!   [sp+0]  = argc
//!   [sp+8]  = argv[0]
//!   [sp+16] = argv[1]
//!   ...
//!   [sp+8*(argc+1)] = NULL
//!   [sp+8*(argc+2)] = envp[0]
//!   ...
//!
//! — NeonRoot

/// Entry point for ARM64 userspace programs
///
/// Reads argc/argv from stack, initializes environment, then calls main().
#[unsafe(no_mangle)]
#[unsafe(naked)]
pub unsafe extern "C" fn _start() -> ! {
    core::arch::naked_asm!(
        // Read argc from stack into x19 (callee-saved)
        "ldr x19, [sp]",
        // Calculate argv pointer (sp + 8) into x20 (callee-saved)
        "add x20, sp, #8",
        // Call init_env to set up environment
        "bl {init_env}",
        // Initialize FILE streams (stdin/stdout/stderr)
        "bl {init_stdio}",
        // Initialize environ pointer for C code
        "bl {init_environ}",
        // Set up arguments for main(argc, argv)
        "mov w0, w19",           // argc (32-bit)
        "mov x1, x20",           // argv
        // Call main(argc, argv)
        "bl {main}",
        // Exit with return code (in w0 from main)
        // w0 already has the return value
        "bl {exit}",
        // Should never reach here
        "brk #0",
        init_env = sym crate::env::init_env,
        init_stdio = sym crate::filestream::init_stdio,
        init_environ = sym crate::c_exports::init_environ,
        main = sym _main_wrapper,
        exit = sym crate::syscall::sys_exit,
    )
}

// Wrapper to call the user's main function with argc/argv
#[inline(never)]
fn _main_wrapper(argc: i32, argv: *const *const u8) -> i32 {
    unsafe extern "Rust" {
        fn main(argc: i32, argv: *const *const u8) -> i32;
    }
    unsafe { main(argc, argv) }
}
