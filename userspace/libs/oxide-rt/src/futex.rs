//! Futex operations — the foundation of userspace synchronization.
//!
//! — ThreadRogue: Fast Userspace muTEXes. The kernel only gets involved
//! when there's actual contention. When things are uncontended, it's
//! just an atomic compare-and-swap. Beautiful, really.
//!
//! std's Mutex, Condvar, RwLock, Once, and thread parking all build on these.

use crate::syscall::*;
use crate::nr;
use crate::types::futex_op;
use core::sync::atomic::AtomicU32;

/// futex_wait — atomically check *addr == expected, then sleep.
/// Returns 0 on wake, -EAGAIN if value changed, -EINTR if interrupted.
pub fn futex_wait(addr: &AtomicU32, expected: u32) -> i32 {
    syscall4(
        nr::FUTEX,
        addr as *const AtomicU32 as usize,
        futex_op::FUTEX_WAIT_PRIVATE as usize,
        expected as usize,
        0, // timeout = NULL (wait forever)
    ) as i32
}

/// futex_wait_timeout — wait with timeout (in Timespec)
pub fn futex_wait_timeout(
    addr: &AtomicU32,
    expected: u32,
    timeout: &crate::types::Timespec,
) -> i32 {
    syscall4(
        nr::FUTEX,
        addr as *const AtomicU32 as usize,
        futex_op::FUTEX_WAIT_PRIVATE as usize,
        expected as usize,
        timeout as *const crate::types::Timespec as usize,
    ) as i32
}

/// futex_wake — wake up to `count` threads waiting on addr.
/// Returns number of threads woken.
pub fn futex_wake(addr: &AtomicU32, count: u32) -> i32 {
    syscall3(
        nr::FUTEX,
        addr as *const AtomicU32 as usize,
        futex_op::FUTEX_WAKE_PRIVATE as usize,
        count as usize,
    ) as i32
}

/// futex_wake_all — wake all waiters
pub fn futex_wake_all(addr: &AtomicU32) -> i32 {
    futex_wake(addr, i32::MAX as u32)
}
