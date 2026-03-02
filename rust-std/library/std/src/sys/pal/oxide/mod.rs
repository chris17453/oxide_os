//! — IronGhost: OXIDE OS Platform Abstraction Layer.
//! Where Rust's std meets our syscalls. No moto_rt, no libc — just oxide_rt.

#![allow(unsafe_op_in_unsafe_fn)]

pub mod os;
pub mod time;

pub mod futex {
    //! — IronGhost: Futex adapter layer. std's sync primitives expect
    //! specific types (Futex, Primitive) and a Duration-based API.
    //! We bridge oxide_rt's raw syscall wrappers to match.

    use crate::sync::atomic::Atomic;
    use crate::time::Duration;

    pub type Futex = Atomic<Primitive>;
    pub type Primitive = u32;
    pub type SmallFutex = Atomic<SmallPrimitive>;
    pub type SmallPrimitive = u32;

    /// Wait on a futex. Returns false on timeout, true otherwise.
    pub fn futex_wait(futex: &Atomic<u32>, expected: u32, timeout: Option<Duration>) -> bool {
        match timeout {
            None => {
                // — IronGhost: Block forever (or until woken/value changes)
                let ret = oxide_rt::futex::futex_wait(futex, expected);
                // EAGAIN (-11) means value changed — that's a valid wakeup
                ret == 0 || ret == -11
            }
            Some(dur) => {
                let ts = oxide_rt::types::Timespec {
                    tv_sec: dur.as_secs() as i64,
                    tv_nsec: dur.subsec_nanos() as i64,
                };
                let ret = oxide_rt::futex::futex_wait_timeout(futex, expected, &ts);
                // -ETIMEDOUT (-110) = timeout, return false
                if ret == -110 { false } else { true }
            }
        }
    }

    /// Wake one waiter.
    pub fn futex_wake(futex: &Atomic<u32>) -> bool {
        oxide_rt::futex::futex_wake(futex, 1) > 0
    }

    /// Wake all waiters.
    pub fn futex_wake_all(futex: &Atomic<u32>) {
        oxide_rt::futex::futex_wake_all(futex);
    }
}

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
