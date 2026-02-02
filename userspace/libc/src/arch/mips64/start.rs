//! MIPS64 (SGI) userspace entry point
//!
//! Entry point for MIPS64 userspace programs.
//! Stack layout at entry (set up by exec):
//!   [sp+0]  = argc
//!   [sp+8]  = argv[0]
//!   [sp+16] = argv[1]
//!   ...
//!   [sp+8*(argc+1)] = NULL
//!   [sp+8*(argc+2)] = envp[0]
//!   ...
//!
//! ⚠️ BIG-ENDIAN: Data structures from firmware/disk are big-endian
//!
//! — GraveShift

/// Entry point for MIPS64 userspace programs
///
/// Reads argc/argv from stack, initializes environment, then calls main().
#[unsafe(no_mangle)]
#[unsafe(naked)]
pub unsafe extern "C" fn _start() -> ! {
    core::arch::naked_asm!(
        // Read argc from stack into $s0 (callee-saved)
        "ld $16, 0($29)",        // $s0 = [$sp], argc
        // Calculate argv pointer (sp + 8) into $s1 (callee-saved)
        "daddiu $17, $29, 8",    // $s1 = $sp + 8, argv pointer
        // Call init_env to set up environment
        "jal {init_env}",
        "nop",                   // delay slot
        // Initialize FILE streams (stdin/stdout/stderr)
        "jal {init_stdio}",
        "nop",
        // Initialize environ pointer for C code
        "jal {init_environ}",
        "nop",
        // Set up arguments for main(argc, argv)
        "move $4, $16",          // $a0 = argc (from $s0)
        "move $5, $17",          // $a1 = argv (from $s1)
        // Call main(argc, argv)
        "jal {main}",
        "nop",
        // Exit with return code (in $v0 from main)
        "move $4, $2",           // $a0 = return value from main
        "jal {exit}",
        "nop",
        // Should never reach here
        "break 0",
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
