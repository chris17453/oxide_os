//! — IronGhost: OXIDE OS Platform Abstraction Layer.
//! Where Rust's std meets our syscalls. No moto_rt, no libc — just oxide_rt.

#![allow(unsafe_op_in_unsafe_fn)]

pub mod os;
pub mod time;

pub use oxide_rt::futex;

use crate::io;

/// Map a raw errno (positive i32) to an io::Error
pub(crate) fn map_oxide_error(errno: i32) -> io::Error {
    io::Error::from_raw_os_error(errno)
}

/// Check a syscall return value and convert to io::Result
pub(crate) fn cvt(ret: i64) -> io::Result<usize> {
    if ret < 0 {
        Err(map_oxide_error((-ret) as i32))
    } else {
        Ok(ret as usize)
    }
}

/// Check a syscall return value (i32 version)
pub(crate) fn cvt_i32(ret: i32) -> io::Result<i32> {
    if ret < 0 {
        Err(map_oxide_error(-ret))
    } else {
        Ok(ret)
    }
}

// SAFETY: must be called only once during runtime initialization.
pub unsafe fn init(_argc: isize, _argv: *const *const u8, _sigpipe: u8) {
    // oxide_rt::start::_start handles init — nothing needed here
}

// SAFETY: must be called only once during runtime cleanup.
pub unsafe fn cleanup() {}

pub fn unsupported<T>() -> io::Result<T> {
    Err(unsupported_err())
}

pub fn unsupported_err() -> io::Error {
    io::Error::UNSUPPORTED_PLATFORM
}

pub fn abort_internal() -> ! {
    core::intrinsics::abort();
}
