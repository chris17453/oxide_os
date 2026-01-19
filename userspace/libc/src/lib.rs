//! EFFLUX C Library (libc)
//!
//! Provides a minimal POSIX-like API for EFFLUX userspace programs.
//! Written in Rust but provides C-compatible interfaces.

#![no_std]
#![allow(unsafe_op_in_unsafe_fn)]

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

// Explicitly re-export to avoid conflicts
pub use stdio::{print, println, eprint, eprintln, putchar, getchar, print_u64, print_i64, print_hex, getline, itoa, atoi, parse_int};
pub use unistd::{write, read, open, open2, close, fork, exec, wait, waitpid, getpid, getppid, dup, dup2, _exit, exit, puts, eputs};
pub use unistd::{pipe, chdir, getcwd, lseek, setsid, setpgid, getpgid};
pub use unistd::{WNOHANG, WUNTRACED, WCONTINUED, wifexited, wexitstatus, wifsignaled, wtermsig, wifstopped, wstopsig};
pub use unistd::{SEEK_SET, SEEK_CUR, SEEK_END};
pub use env::{setenv, unsetenv, getenv, init_env, env_iter};

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
/// This must be a naked function to avoid compiler-generated prologues
/// that would misalign the stack. At program entry, RSP is 16-byte aligned.
/// The System V ABI requires RSP % 16 == 0 before a `call` instruction,
/// which is already satisfied.
#[unsafe(no_mangle)]
#[unsafe(naked)]
pub unsafe extern "C" fn _start() -> ! {
    core::arch::naked_asm!(
        // RSP is already 16-byte aligned at entry
        // Call init_env to set up environment
        "call {init_env}",
        // Call main
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

// Wrapper to call the user's main function
#[inline(never)]
fn _main_wrapper() -> i32 {
    unsafe extern "Rust" {
        fn main() -> i32;
    }
    unsafe { main() }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    syscall::sys_exit(1);
}
