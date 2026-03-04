//! Futex operations — the foundation of userspace synchronization.
//!
//! — ThreadRogue: Fast Userspace muTEXes. The kernel only gets involved
//! when there's actual contention. When things are uncontended, it's
//! just an atomic compare-and-swap. Beautiful, really.
//!
//! std's Mutex, Condvar, RwLock, Once, and thread parking all build on these.
//! The public API matches what std::sys::sync expects — same signatures as
//! the unix futex module. The raw syscall wrappers are internal.

use crate::syscall::*;
use crate::nr;
use crate::types::{futex_op, Timespec};
use core::sync::atomic::{AtomicU32, Ordering};
use core::time::Duration;

// — ThreadRogue: type aliases that std's sync primitives expect.
// std::sys::futex::{Futex, Primitive, SmallFutex, SmallPrimitive}
// These mirror the unix futex module exactly.
pub type Primitive = u32;
pub type Futex = AtomicU32;
pub type SmallPrimitive = u32;
pub type SmallFutex = AtomicU32;

// ── Raw syscall wrappers (internal) ──

fn raw_futex_wait(addr: &AtomicU32, expected: u32, timeout_ptr: usize) -> i32 {
    syscall4(
        nr::FUTEX,
        addr as *const AtomicU32 as usize,
        futex_op::FUTEX_WAIT_PRIVATE as usize,
        expected as usize,
        timeout_ptr,
    ) as i32
}

fn raw_futex_wake(addr: &AtomicU32, count: u32) -> i32 {
    syscall3(
        nr::FUTEX,
        addr as *const AtomicU32 as usize,
        futex_op::FUTEX_WAKE_PRIVATE as usize,
        count as usize,
    ) as i32
}

// ── std-compatible public API ──

/// Waits for a `futex_wake` operation to wake us.
/// Returns directly if the futex doesn't hold the expected value.
/// Returns false on timeout, and true in all other cases.
///
/// — ThreadRogue: Matches std::sys::pal::unix::futex::futex_wait signature exactly.
/// The timeout is converted to a kernel Timespec. None = wait forever.
pub fn futex_wait(futex: &AtomicU32, expected: u32, timeout: Option<Duration>) -> bool {
    let timespec;
    let timeout_ptr = match timeout {
        Some(dur) => {
            timespec = Timespec {
                tv_sec: dur.as_secs() as i64,
                tv_nsec: dur.subsec_nanos() as i64,
            };
            &timespec as *const Timespec as usize
        }
        None => 0, // NULL = wait forever
    };

    loop {
        // — ThreadRogue: no need to wait if the value already changed
        if futex.load(Ordering::Relaxed) != expected {
            return true;
        }

        let r = raw_futex_wait(futex, expected, timeout_ptr);
        // — ThreadRogue: decode the return value like Linux does:
        // 0 = woken normally, -EAGAIN = value changed, -EINTR = signal
        // -ETIMEDOUT = timeout expired
        if r == 0 || r == -11 { // 0 or -EAGAIN
            return true;
        }
        if r == -110 { // -ETIMEDOUT
            return false;
        }
        if r == -4 { // -EINTR — interrupted by signal, retry
            continue;
        }
        // — ThreadRogue: any other error, treat as woken
        return true;
    }
}

/// Wakes up one thread that's blocked on `futex_wait` on this futex.
/// Returns true if this actually woke up such a thread.
///
/// — ThreadRogue: Matches std::sys::pal::unix::futex::futex_wake signature.
pub fn futex_wake(futex: &AtomicU32) -> bool {
    raw_futex_wake(futex, 1) > 0
}

/// Wakes up all threads that are waiting on `futex_wait` on this futex.
///
/// — ThreadRogue: Matches std::sys::pal::unix::futex::futex_wake_all signature.
pub fn futex_wake_all(futex: &AtomicU32) {
    raw_futex_wake(futex, i32::MAX as u32);
}
