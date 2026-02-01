//! OXIDE C Library (libc)
//!
//! Provides a minimal POSIX-like API for OXIDE userspace programs.
//! Written in Rust but provides C-compatible interfaces.

#![no_std]
#![allow(unused)]
#![allow(unsafe_op_in_unsafe_fn)]
#![feature(c_variadic)]

extern crate alloc;

// Bump allocator with small BSS bootstrap + mmap-backed growth.
//
// The bootstrap heap (256KB in BSS) handles allocations during _start before
// syscalls are available. Once it fills up, additional memory is obtained via
// mmap in 2MB arenas, up to 64MB total. This avoids putting a giant 64MB
// static array in BSS which would exhaust physical memory during exec.
mod allocator {
    use core::alloc::{GlobalAlloc, Layout};
    use core::cell::UnsafeCell;
    use core::sync::atomic::{AtomicUsize, Ordering};

    /// Small bootstrap heap in BSS — enough for _start / init_env / init_stdio.
    const BOOTSTRAP_SIZE: usize = 256 * 1024; // 256KB

    /// Size of each mmap arena.
    const ARENA_SIZE: usize = 2 * 1024 * 1024; // 2MB

    /// Maximum number of mmap arenas (2MB × 32 = 64MB ceiling).
    const MAX_ARENAS: usize = 32;

    #[repr(C, align(16))]
    struct BootstrapHeap {
        data: UnsafeCell<[u8; BOOTSTRAP_SIZE]>,
    }

    unsafe impl Sync for BootstrapHeap {}

    static BOOTSTRAP: BootstrapHeap = BootstrapHeap {
        data: UnsafeCell::new([0; BOOTSTRAP_SIZE]),
    };
    static BOOTSTRAP_POS: AtomicUsize = AtomicUsize::new(0);

    /// Current mmap arena base address (0 = no arena allocated yet).
    static ARENA_BASE: AtomicUsize = AtomicUsize::new(0);
    /// Current bump position within the arena.
    static ARENA_POS: AtomicUsize = AtomicUsize::new(0);
    /// Number of arenas allocated so far.
    static ARENA_COUNT: AtomicUsize = AtomicUsize::new(0);

    pub struct BumpAllocator;

    unsafe impl GlobalAlloc for BumpAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let size = layout.size();
            let align = layout.align();

            // ── Try bootstrap heap first ────────────────────────────
            let pos = BOOTSTRAP_POS.load(Ordering::Relaxed);
            let aligned = (pos + align - 1) & !(align - 1);
            let new_pos = aligned + size;

            if new_pos <= BOOTSTRAP_SIZE {
                // CAS for safety (single-threaded in practice, but correct)
                if BOOTSTRAP_POS
                    .compare_exchange(pos, new_pos, Ordering::SeqCst, Ordering::Relaxed)
                    .is_ok()
                {
                    return (BOOTSTRAP.data.get() as *mut u8).add(aligned);
                }
                // Lost race — fall through to retry via arena path
            }

            // ── Bootstrap full — allocate from mmap arena ──────────
            self.arena_alloc(size, align)
        }

        unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
            // Bump allocator doesn't free — memory is reclaimed on process exit
        }
    }

    impl BumpAllocator {
        /// Allocate from the current mmap arena, creating a new one if needed.
        unsafe fn arena_alloc(&self, size: usize, align: usize) -> *mut u8 {
            loop {
                let base = ARENA_BASE.load(Ordering::Acquire);
                if base == 0 {
                    if !Self::new_arena() {
                        return core::ptr::null_mut();
                    }
                    continue;
                }

                let pos = ARENA_POS.load(Ordering::Relaxed);
                let aligned = (pos + align - 1) & !(align - 1);
                let new_pos = aligned + size;

                if new_pos > ARENA_SIZE {
                    // Current arena exhausted — allocate a fresh one
                    if !Self::new_arena() {
                        return core::ptr::null_mut();
                    }
                    continue;
                }

                if ARENA_POS
                    .compare_exchange_weak(pos, new_pos, Ordering::SeqCst, Ordering::Relaxed)
                    .is_ok()
                {
                    return (base as *mut u8).add(aligned);
                }
            }
        }

        /// Allocate a new mmap arena.
        fn new_arena() -> bool {
            let count = ARENA_COUNT.load(Ordering::Relaxed);
            if count >= MAX_ARENAS {
                return false;
            }

            let ptr = unsafe {
                crate::syscall::sys_mmap(
                    core::ptr::null_mut(),
                    ARENA_SIZE,
                    0x3,  // PROT_READ | PROT_WRITE
                    0x22, // MAP_PRIVATE | MAP_ANONYMOUS
                    -1,
                    0,
                )
            };

            if ptr.is_null() || ptr as usize == usize::MAX {
                return false;
            }

            // Publish the new arena — ordering matters: set pos before base
            // so readers see pos=0 when they observe the new base.
            ARENA_POS.store(0, Ordering::Release);
            ARENA_BASE.store(ptr as usize, Ordering::Release);
            ARENA_COUNT.fetch_add(1, Ordering::SeqCst);
            true
        }
    }
}

#[global_allocator]
static ALLOCATOR: allocator::BumpAllocator = allocator::BumpAllocator;

// Architecture-specific implementations
pub mod arch;

// Core modules
pub mod env;
pub mod errno;
pub mod fcntl;
pub mod getopt;
pub mod signal;
pub mod stat;
pub mod stdio;
pub mod string;
pub mod syscall;
pub mod unistd;

pub mod readline;
pub mod syslog;

// Extended POSIX modules
pub mod dirent;
pub mod dlfcn;
pub mod dns;
pub mod locale;
pub mod math;
pub mod poll;
pub mod pwd;
pub mod socket;
pub mod termios;
pub mod time;
pub mod wchar;

// CPython support modules
pub mod c_exports;
pub mod ctype;
pub mod filestream;
pub mod math_exports;
pub mod printf;
pub mod setjmp;

pub use errno::*;
pub use fcntl::*;
pub use signal::*;
pub use string::*;
pub use syscall::*;

// Explicitly re-export stdio functions
// Note: print!, println!, eprint!, eprintln! macros exist for formatted output
// Use the `prints`, `printlns`, `eprints`, `eprintlns` functions for simple string printing
// to avoid macro name conflicts
pub use env::{env_iter, getenv, init_env, setenv, unsetenv};
pub use stdio::{
    StderrWriter, StdoutWriter, atoi, getchar, getline, itoa, parse_int, print_hex, print_i64,
    print_u64, putchar,
};
pub use stdio::{eprintlns, eprints, printlns, prints};
pub use unistd::{
    _exit, close, dup, dup2, eputs, exec, execv, execve, exit, fork, getpid, getppid, open, open2,
    puts, read, wait, waitpid, write,
};
pub use unistd::{F_OK, R_OK, W_OK, X_OK};
pub use unistd::{SEEK_CUR, SEEK_END, SEEK_SET};
pub use unistd::{
    WCONTINUED, WNOHANG, WUNTRACED, wexitstatus, wifexited, wifsignaled, wifstopped, wstopsig,
    wtermsig,
};
pub use unistd::{
    access, fpathconf, ftruncate, gethostname, getlogin, getlogin_r, isatty, pathconf, realpath,
    setpgrp, sysconf, system, truncate, ttyname,
};
pub use unistd::{
    chdir, getcwd, getpgid, lseek, pipe, sched_yield, setpgid, setsid, tcgetpgrp, tcsetpgrp,
};

pub use syslog::{
    LOG_ALERT, LOG_CRIT, LOG_DEBUG, LOG_EMERG, LOG_ERR, LOG_INFO, LOG_NOTICE, LOG_WARNING,
};
pub use syslog::{closelog, openlog, setlogmask, syslog};

// Stat functions
pub use stat::{
    S_IFBLK, S_IFCHR, S_IFDIR, S_IFIFO, S_IFLNK, S_IFMT, S_IFREG, S_IFSOCK, Stat, fstat, lstat,
    stat,
};

// User/group functions
pub use pwd::{getegid, geteuid, getgid, getuid, setegid, seteuid, setgid, setuid};

// Time functions
pub use time::clocks;
pub use time::{
    Timespec, Timeval, Timezone, Tm, clock, clock_getres, clock_gettime, gettimeofday, gmtime_r,
    mktime, nanosleep, sleep, time, usleep,
};

// System info functions
pub use syscall::{Statfs, UtsName, fstatfs, statfs, sys_uname, uname};

// Additional syscall wrappers
pub use syscall::{sys_getdents as getdents, sys_gettid as gettid};
pub use syscall::{
    sys_kill as kill, sys_mkdir as mkdir, sys_rename as rename, sys_rmdir as rmdir,
    sys_unlink as unlink,
};

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
        // Initialize FILE streams (stdin/stdout/stderr)
        "call {init_stdio}",
        // Initialize environ pointer for C code
        "call {init_environ}",
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
        init_stdio = sym filestream::init_stdio,
        init_environ = sym c_exports::init_environ,
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
