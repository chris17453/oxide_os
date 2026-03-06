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
//! — GraveShift: MUST be naked — any prologue (push rbp) corrupts RSP
//!   before we read argc from [rsp]. The kernel sets RSP to point directly
//!   at argc on the stack via sysretq. No frame setup allowed.

/// Entry point for x86_64 userspace programs
///
/// Reads argc/argv from stack per x86_64 ELF ABI, initializes environment,
/// calls main(), and exits with the return code.
#[unsafe(no_mangle)]
#[unsafe(naked)]
pub unsafe extern "C" fn _start() -> ! {
    // GraveShift: naked entry — RSP points at argc, no prologue allowed.
    // Stash argc/argv in callee-saved regs (r12/r13) across init calls.
    core::arch::naked_asm!(
        // Save argc and argv in callee-saved registers
        "mov r12d, [rsp]",          // argc (32-bit)
        "lea r13, [rsp + 8]",       // argv = &stack[1]
        // 16-byte align the stack for System V ABI calls
        "and rsp, -16",
        // Initialize environment
        "call {init_env}",
        // Initialize FILE streams (stdin/stdout/stderr)
        "call {init_stdio}",
        // Initialize environ pointer for C compatibility
        "call {init_environ}",
        // Set up arguments for main(argc, argv)
        "mov edi, r12d",            // argc (32-bit)
        "mov rsi, r13",             // argv
        // Call main(argc, argv)
        "call {main}",
        // Exit with return code (in eax from main)
        "mov edi, eax",
        "call {exit}",
        // Should never reach here
        "ud2",
        init_env = sym crate::env::init_env,
        init_stdio = sym crate::filestream::init_stdio,
        init_environ = sym crate::c_exports::init_environ,
        main = sym _main_wrapper,
        exit = sym crate::syscall::sys_exit,
    )
}

// — GraveShift: Wrapper must use C ABI — Python and other clang-compiled binaries
// export main() with C calling convention. Rust programs on x86_64 pass (i32, ptr)
// identically through both ABIs, so this is safe for everyone.
#[inline(never)]
fn _main_wrapper(argc: i32, argv: *const *const u8) -> i32 {
    unsafe extern "C" {
        fn main(argc: i32, argv: *const *const u8) -> i32;
    }
    unsafe { main(argc, argv) }
}
