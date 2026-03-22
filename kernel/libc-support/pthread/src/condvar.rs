//! Condition variable implementation
//! — BlackLatch: condvars with real futex backing, no more spin-praying

use core::ffi::c_int;
use core::sync::atomic::{AtomicU32, Ordering};

/// Futex wait — blocks if *addr == expected. No timeout.
/// — BlackLatch: park the thread in the kernel, let the scheduler sort it out
unsafe fn futex_wait(addr: *const u32, expected: u32) {
    let _: isize;
    core::arch::asm!("syscall",
        in("rax") 202u64,
        in("rdi") addr as usize,
        in("rsi") 0usize,  // FUTEX_WAIT
        in("rdx") expected as usize,
        in("r10") 0usize,  // no timeout
        lateout("rax") _,
        out("rcx") _, out("r11") _);
}

/// Futex wake — wakes up to `count` waiters.
/// — BlackLatch: wakey wakey, someone signaled
unsafe fn futex_wake(addr: *const u32, count: u32) {
    let _: isize;
    core::arch::asm!("syscall",
        in("rax") 202u64,
        in("rdi") addr as usize,
        in("rsi") 1usize,  // FUTEX_WAKE
        in("rdx") count as usize,
        in("r10") 0usize,
        lateout("rax") _,
        out("rcx") _, out("r11") _);
}

/// Futex wait with timeout — blocks if *addr == expected, wakes on timeout.
/// — BlackLatch: patience has limits, even for threads
unsafe fn futex_wait_timeout(addr: *const u32, expected: u32, timeout: *const timespec) {
    let _: isize;
    core::arch::asm!("syscall",
        in("rax") 202u64,
        in("rdi") addr as usize,
        in("rsi") 0usize,  // FUTEX_WAIT
        in("rdx") expected as usize,
        in("r10") timeout as usize,
        lateout("rax") _,
        out("rcx") _, out("r11") _);
}

use crate::mutex::{pthread_mutex_lock, pthread_mutex_t, pthread_mutex_unlock};
use crate::{EINVAL, ESUCCESS, ETIMEDOUT};

/// Condition variable structure
#[repr(C)]
pub struct pthread_cond_t {
    /// Sequence number for wake/wait coordination
    seq: AtomicU32,
    /// Number of waiters
    waiters: AtomicU32,
}

impl pthread_cond_t {
    /// Static initializer
    pub const INITIALIZER: Self = Self {
        seq: AtomicU32::new(0),
        waiters: AtomicU32::new(0),
    };
}

/// Condition variable attributes
#[repr(C)]
pub struct pthread_condattr_t {
    /// Process sharing
    pub pshared: c_int,
    /// Clock ID
    pub clock_id: c_int,
}

impl pthread_condattr_t {
    pub const fn new() -> Self {
        Self {
            pshared: 0,
            clock_id: 0, // CLOCK_REALTIME
        }
    }
}

/// Timespec structure for timed wait
#[repr(C)]
pub struct timespec {
    pub tv_sec: i64,
    pub tv_nsec: i64,
}

/// Initialize condition variable
#[no_mangle]
pub unsafe extern "C" fn pthread_cond_init(
    cond: *mut pthread_cond_t,
    _attr: *const pthread_condattr_t,
) -> c_int {
    if cond.is_null() {
        return EINVAL;
    }

    (*cond).seq.store(0, Ordering::SeqCst);
    (*cond).waiters.store(0, Ordering::SeqCst);

    ESUCCESS
}

/// Destroy condition variable
#[no_mangle]
pub unsafe extern "C" fn pthread_cond_destroy(cond: *mut pthread_cond_t) -> c_int {
    if cond.is_null() {
        return EINVAL;
    }

    // Check for waiters
    if (*cond).waiters.load(Ordering::SeqCst) > 0 {
        return EINVAL;
    }

    ESUCCESS
}

/// Wait on condition variable
#[no_mangle]
pub unsafe extern "C" fn pthread_cond_wait(
    cond: *mut pthread_cond_t,
    mutex: *mut pthread_mutex_t,
) -> c_int {
    if cond.is_null() || mutex.is_null() {
        return EINVAL;
    }

    // Get current sequence number
    let seq = (*cond).seq.load(Ordering::SeqCst);

    // Increment waiter count
    (*cond).waiters.fetch_add(1, Ordering::SeqCst);

    // Release mutex
    pthread_mutex_unlock(mutex);

    // — BlackLatch: real futex wait on the sequence counter, sleep until signaled
    loop {
        let current = (*cond).seq.load(Ordering::SeqCst);
        if current != seq {
            break;
        }
        futex_wait((*cond).seq.as_ptr(), seq);
    }

    // Decrement waiter count
    (*cond).waiters.fetch_sub(1, Ordering::SeqCst);

    // Reacquire mutex
    pthread_mutex_lock(mutex);

    ESUCCESS
}

/// Wait on condition variable with timeout
#[no_mangle]
pub unsafe extern "C" fn pthread_cond_timedwait(
    cond: *mut pthread_cond_t,
    mutex: *mut pthread_mutex_t,
    abstime: *const timespec,
) -> c_int {
    if cond.is_null() || mutex.is_null() || abstime.is_null() {
        return EINVAL;
    }

    // Get current sequence number
    let seq = (*cond).seq.load(Ordering::SeqCst);

    // Increment waiter count
    (*cond).waiters.fetch_add(1, Ordering::SeqCst);

    // Release mutex
    pthread_mutex_unlock(mutex);

    // — BlackLatch: futex wait with real kernel timeout, no more fake iteration counters
    loop {
        let current = (*cond).seq.load(Ordering::SeqCst);
        if current != seq {
            break;
        }
        futex_wait_timeout((*cond).seq.as_ptr(), seq, abstime);
        // Check if we were woken by timeout (seq unchanged means timeout)
        let after = (*cond).seq.load(Ordering::SeqCst);
        if after == seq {
            // Timed out — seq hasn't changed
            (*cond).waiters.fetch_sub(1, Ordering::SeqCst);
            pthread_mutex_lock(mutex);
            return ETIMEDOUT;
        }
    }

    // Decrement waiter count
    (*cond).waiters.fetch_sub(1, Ordering::SeqCst);

    // Reacquire mutex
    pthread_mutex_lock(mutex);

    ESUCCESS
}

/// Signal one waiter
#[no_mangle]
pub unsafe extern "C" fn pthread_cond_signal(cond: *mut pthread_cond_t) -> c_int {
    if cond.is_null() {
        return EINVAL;
    }

    // Only signal if there are waiters
    if (*cond).waiters.load(Ordering::SeqCst) > 0 {
        // Increment sequence to wake one waiter
        (*cond).seq.fetch_add(1, Ordering::SeqCst);
        // — BlackLatch: tap one thread on the shoulder via futex
        futex_wake((*cond).seq.as_ptr(), 1);
    }

    ESUCCESS
}

/// Signal all waiters
#[no_mangle]
pub unsafe extern "C" fn pthread_cond_broadcast(cond: *mut pthread_cond_t) -> c_int {
    if cond.is_null() {
        return EINVAL;
    }

    // Only signal if there are waiters
    if (*cond).waiters.load(Ordering::SeqCst) > 0 {
        // Increment sequence to wake all waiters
        (*cond).seq.fetch_add(1, Ordering::SeqCst);
        // — BlackLatch: everybody up, broadcast means EVERYBODY
        futex_wake((*cond).seq.as_ptr(), i32::MAX as u32);
    }

    ESUCCESS
}

/// Initialize condition variable attributes
#[no_mangle]
pub unsafe extern "C" fn pthread_condattr_init(attr: *mut pthread_condattr_t) -> c_int {
    if attr.is_null() {
        return EINVAL;
    }
    *attr = pthread_condattr_t::new();
    ESUCCESS
}

/// Destroy condition variable attributes
#[no_mangle]
pub unsafe extern "C" fn pthread_condattr_destroy(attr: *mut pthread_condattr_t) -> c_int {
    if attr.is_null() {
        return EINVAL;
    }
    ESUCCESS
}

/// Set clock ID
#[no_mangle]
pub unsafe extern "C" fn pthread_condattr_setclock(
    attr: *mut pthread_condattr_t,
    clock_id: c_int,
) -> c_int {
    if attr.is_null() {
        return EINVAL;
    }
    (*attr).clock_id = clock_id;
    ESUCCESS
}

/// Get clock ID
#[no_mangle]
pub unsafe extern "C" fn pthread_condattr_getclock(
    attr: *const pthread_condattr_t,
    clock_id: *mut c_int,
) -> c_int {
    if attr.is_null() || clock_id.is_null() {
        return EINVAL;
    }
    *clock_id = (*attr).clock_id;
    ESUCCESS
}
