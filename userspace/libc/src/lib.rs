//! OXIDE C Library (libc)
//!
//! Provides a minimal POSIX-like API for OXIDE userspace programs.
//! Written in Rust but provides C-compatible interfaces.

#![no_std]
#![allow(unsafe_op_in_unsafe_fn)]

extern crate alloc;

// Simple bump allocator for userspace
mod allocator {
    use core::alloc::{GlobalAlloc, Layout};
    use core::cell::UnsafeCell;
    use core::sync::atomic::{AtomicUsize, Ordering};

    const HEAP_SIZE: usize = 1024 * 1024; // 1MB heap

    #[repr(C, align(16))]
    struct HeapStorage {
        data: UnsafeCell<[u8; HEAP_SIZE]>,
    }

    unsafe impl Sync for HeapStorage {}

    static HEAP: HeapStorage = HeapStorage {
        data: UnsafeCell::new([0; HEAP_SIZE]),
    };
    static HEAP_POS: AtomicUsize = AtomicUsize::new(0);

    pub struct BumpAllocator;

    unsafe impl GlobalAlloc for BumpAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let size = layout.size();
            let align = layout.align();

            loop {
                let pos = HEAP_POS.load(Ordering::Relaxed);
                let aligned = (pos + align - 1) & !(align - 1);
                let new_pos = aligned + size;

                if new_pos > HEAP_SIZE {
                    return core::ptr::null_mut();
                }

                if HEAP_POS.compare_exchange_weak(pos, new_pos, Ordering::SeqCst, Ordering::Relaxed).is_ok() {
                    return (HEAP.data.get() as *mut u8).add(aligned);
                }
            }
        }

        unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
            // Bump allocator doesn't free - memory is reclaimed when process exits
        }
    }
}

#[global_allocator]
static ALLOCATOR: allocator::BumpAllocator = allocator::BumpAllocator;

// Architecture-specific implementations
pub mod arch;

// Core modules
pub mod errno;
pub mod fcntl;
pub mod signal;
pub mod string;
pub mod syscall;
pub mod unistd;
pub mod stdio;
pub mod env;
pub mod stat;

// Extended POSIX modules
pub mod dirent;
pub mod time;
pub mod dlfcn;
pub mod locale;
pub mod wchar;
pub mod math;
pub mod poll;
pub mod termios;
pub mod pwd;
pub mod socket;
pub mod dns;

pub use errno::*;
pub use fcntl::*;
pub use signal::*;
pub use string::*;
pub use syscall::*;

// Explicitly re-export stdio functions
// Note: print!, println!, eprint!, eprintln! macros exist for formatted output
// Use the `prints`, `printlns`, `eprints`, `eprintlns` functions for simple string printing
// to avoid macro name conflicts
pub use stdio::{StdoutWriter, StderrWriter, putchar, getchar, print_u64, print_i64, print_hex, getline, itoa, atoi, parse_int};
pub use stdio::{prints, printlns, eprints, eprintlns};
pub use unistd::{write, read, open, open2, close, fork, exec, execv, execve, wait, waitpid, getpid, getppid, dup, dup2, _exit, exit, puts, eputs};
pub use unistd::{pipe, chdir, getcwd, lseek, setsid, setpgid, getpgid};
pub use unistd::{WNOHANG, WUNTRACED, WCONTINUED, wifexited, wexitstatus, wifsignaled, wtermsig, wifstopped, wstopsig};
pub use unistd::{SEEK_SET, SEEK_CUR, SEEK_END};
pub use env::{setenv, unsetenv, getenv, init_env, env_iter};

// Stat functions
pub use stat::{stat, fstat, lstat, Stat, S_IFMT, S_IFREG, S_IFDIR, S_IFLNK, S_IFCHR, S_IFBLK, S_IFIFO, S_IFSOCK};

// User/group functions
pub use pwd::{getuid, getgid, geteuid, getegid, setuid, setgid, seteuid, setegid};

// Additional syscall wrappers
pub use syscall::{sys_kill as kill, sys_mkdir as mkdir, sys_rmdir as rmdir, sys_unlink as unlink, sys_rename as rename};
pub use syscall::{sys_gettid as gettid, sys_getdents as getdents};

/// Global errno variable
static mut ERRNO: i32 = 0;

/// Get errno
pub fn get_errno() -> i32 {
    unsafe { ERRNO }
}

/// Set errno
pub fn set_errno(e: i32) {
    unsafe { ERRNO = e }
}

/// Entry point for userspace programs
///
/// Stack layout at entry (set up by exec):
///   [rsp+0]  = argc
///   [rsp+8]  = argv[0]
///   [rsp+16] = argv[1]
///   ...
///   [rsp+8*(argc+1)] = NULL
///   [rsp+8*(argc+2)] = envp[0]
///   ...
///
/// We read argc/argv from the stack, not registers, for robustness.
#[unsafe(no_mangle)]
#[unsafe(naked)]
pub unsafe extern "C" fn _start() -> ! {
    core::arch::naked_asm!(
        // Read argc from stack
        "mov r12, [rsp]",        // argc -> r12 (callee-saved)
        // Calculate argv pointer (rsp + 8)
        "lea r13, [rsp + 8]",    // argv -> r13 (callee-saved)
        // Call init_env to set up environment
        "call {init_env}",
        // Set up arguments for main(argc, argv)
        "mov edi, r12d",         // argc (32-bit)
        "mov rsi, r13",          // argv
        // Call main(argc, argv)
        "call {main}",
        // Exit with return code (in eax from main)
        "mov edi, eax",
        "call {exit}",
        // Should never reach here, but just in case
        "ud2",
        init_env = sym env::init_env,
        main = sym _main_wrapper,
        exit = sym syscall::sys_exit,
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

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    syscall::sys_exit(1);
}
