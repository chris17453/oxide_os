//! Program entry point — the first userspace code that runs.
//!
//! — GraveShift: _start is naked because ANY prologue (push rbp) corrupts
//! RSP before we read argc from [rsp]. The kernel sets RSP to point
//! directly at argc via sysretq. No frame setup allowed. Period.
//!
//! Stack layout at entry (set up by kernel's exec):
//!   [rsp+0]            = argc
//!   [rsp+8]            = argv[0]
//!   [rsp+8*(1)]        = argv[1]
//!   ...
//!   [rsp+8*(argc)]     = argv[argc-1]
//!   [rsp+8*(argc+1)]   = NULL (argv terminator)
//!   [rsp+8*(argc+2)]   = envp[0]
//!   ...
//!   = NULL (envp terminator)

use core::arch::asm;

/// Entry point for OXIDE userspace programs using Rust std.
///
/// — GraveShift: Reads argc/argv/envp from stack, stores them for std,
/// then calls the compiler-generated `main` which calls `lang_start`
/// which initializes std and calls the user's `fn main()`.
#[unsafe(no_mangle)]
#[unsafe(naked)]
pub unsafe extern "C" fn _start() -> ! {
    unsafe {
        core::arch::naked_asm!(
            // — GraveShift: r12 = argc, r13 = argv (callee-saved, survive function calls)
            "mov r12, [rsp]",        // argc (64-bit for isize)
            "lea r13, [rsp + 8]",    // argv = &stack[1]
            "and rsp, -16",          // align stack to 16 bytes (ABI requirement)

            // — GraveShift: Store argc/argv for oxide_rt::args
            "mov rdi, r12",
            "mov rsi, r13",
            "call {set_args}",

            // — GraveShift: Compute envp = argv + (argc + 1) * 8 and init env
            "lea rdi, [r13 + r12*8 + 8]",
            "call {init_env}",

            // — GraveShift: Call the compiler-generated main(argc, argv).
            // For std programs: compiler generates extern "C" fn main(isize, *const *const u8) -> isize
            //   which calls lang_start → sys::init() → user's fn main()
            // For no_std programs using oxide-rt directly: user provides the main symbol
            "mov rdi, r12",
            "mov rsi, r13",
            "call main",

            // — GraveShift: Exit with main's return value
            "mov rdi, rax",
            "call {exit}",
            "ud2",

            set_args = sym crate::args::set_args,
            init_env = sym crate::env::init_from_envp,
            exit = sym crate::os::exit,
        );
    }
}
