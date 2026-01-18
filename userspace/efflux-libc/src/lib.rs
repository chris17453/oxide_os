//! EFFLUX C Library (efflux-libc)
//!
//! Provides a minimal POSIX-like API for EFFLUX userspace programs.
//! Written in Rust but provides C-compatible interfaces.

#![no_std]
#![allow(unsafe_op_in_unsafe_fn)]

pub mod errno;
pub mod fcntl;
pub mod signal;
pub mod string;
pub mod syscall;
pub mod unistd;
pub mod stdio;

pub use errno::*;
pub use fcntl::*;
pub use signal::*;
pub use string::*;
pub use syscall::*;

// Explicitly re-export to avoid conflicts
pub use stdio::{print, println, eprint, eprintln, putchar, getchar, print_u64, print_i64, print_hex, getline, itoa, atoi, parse_int};
pub use unistd::{write, read, open, close, fork, exec, wait, waitpid, getpid, getppid, dup, dup2, _exit, puts, eputs};
pub use unistd::{WNOHANG, WUNTRACED, WCONTINUED, wifexited, wexitstatus, wifsignaled, wtermsig, wifstopped, wstopsig};

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
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _start() -> ! {
    unsafe extern "Rust" {
        fn main() -> i32;
    }

    let ret = unsafe { main() };
    syscall::sys_exit(ret);
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    syscall::sys_exit(1);
}
