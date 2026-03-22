//! Reader-writer lock implementation
//! — ShadePacket: rwlock finally stops melting CPUs, futex edition

use core::ffi::c_int;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// Futex wait — blocks if *addr == expected. No timeout.
/// — ShadePacket: lay down your registers and sleep, the lock will come to you
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
/// — ShadePacket: the lock changed hands, wake the hungry threads
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

use crate::{pthread_self, EBUSY, EDEADLK, EINVAL, ESUCCESS};

/// RWLock state encoding:
/// - High bit (31): write lock held
/// - Lower bits (0-30): reader count
const WRITE_LOCKED: u32 = 1 << 31;
const READER_MASK: u32 = !WRITE_LOCKED;

/// Reader-writer lock structure
#[repr(C)]
pub struct pthread_rwlock_t {
    /// Lock state
    state: AtomicU32,
    /// Writer thread (for debugging/error checking)
    writer: AtomicU64,
    /// Number of waiting writers
    waiting_writers: AtomicU32,
}

impl pthread_rwlock_t {
    /// Static initializer
    pub const INITIALIZER: Self = Self {
        state: AtomicU32::new(0),
        writer: AtomicU64::new(0),
        waiting_writers: AtomicU32::new(0),
    };
}

/// Reader-writer lock attributes
#[repr(C)]
pub struct pthread_rwlockattr_t {
    /// Process sharing
    pub pshared: c_int,
    /// Kind (prefer readers/writers)
    pub kind: c_int,
}

impl pthread_rwlockattr_t {
    pub const fn new() -> Self {
        Self {
            pshared: 0,
            kind: 0,
        }
    }
}

/// Initialize rwlock
#[no_mangle]
pub unsafe extern "C" fn pthread_rwlock_init(
    rwlock: *mut pthread_rwlock_t,
    _attr: *const pthread_rwlockattr_t,
) -> c_int {
    if rwlock.is_null() {
        return EINVAL;
    }

    (*rwlock).state.store(0, Ordering::SeqCst);
    (*rwlock).writer.store(0, Ordering::SeqCst);
    (*rwlock).waiting_writers.store(0, Ordering::SeqCst);

    ESUCCESS
}

/// Destroy rwlock
#[no_mangle]
pub unsafe extern "C" fn pthread_rwlock_destroy(rwlock: *mut pthread_rwlock_t) -> c_int {
    if rwlock.is_null() {
        return EINVAL;
    }

    let state = (*rwlock).state.load(Ordering::SeqCst);
    if state != 0 {
        return EBUSY;
    }

    ESUCCESS
}

/// Acquire read lock
#[no_mangle]
pub unsafe extern "C" fn pthread_rwlock_rdlock(rwlock: *mut pthread_rwlock_t) -> c_int {
    if rwlock.is_null() {
        return EINVAL;
    }

    // — ShadePacket: futex-backed read lock acquisition, sleep when writer holds it
    loop {
        let state = (*rwlock).state.load(Ordering::Acquire);

        // Can't acquire if write-locked or writers waiting
        if (state & WRITE_LOCKED) != 0 || (*rwlock).waiting_writers.load(Ordering::Acquire) > 0 {
            futex_wait((*rwlock).state.as_ptr(), state);
            continue;
        }

        // Try to increment reader count
        let new_state = state + 1;
        if (*rwlock)
            .state
            .compare_exchange_weak(state, new_state, Ordering::AcqRel, Ordering::Relaxed)
            .is_ok()
        {
            return ESUCCESS;
        }
    }
}

/// Try to acquire read lock
#[no_mangle]
pub unsafe extern "C" fn pthread_rwlock_tryrdlock(rwlock: *mut pthread_rwlock_t) -> c_int {
    if rwlock.is_null() {
        return EINVAL;
    }

    let state = (*rwlock).state.load(Ordering::Acquire);

    // Can't acquire if write-locked
    if (state & WRITE_LOCKED) != 0 {
        return EBUSY;
    }

    // Try to increment reader count
    let new_state = state + 1;
    if (*rwlock)
        .state
        .compare_exchange(state, new_state, Ordering::AcqRel, Ordering::Relaxed)
        .is_ok()
    {
        ESUCCESS
    } else {
        EBUSY
    }
}

/// Acquire write lock
#[no_mangle]
pub unsafe extern "C" fn pthread_rwlock_wrlock(rwlock: *mut pthread_rwlock_t) -> c_int {
    if rwlock.is_null() {
        return EINVAL;
    }

    let self_id = pthread_self();

    // Check for recursive write lock (deadlock)
    if (*rwlock).writer.load(Ordering::Acquire) == self_id {
        return EDEADLK;
    }

    // Indicate writer is waiting
    (*rwlock).waiting_writers.fetch_add(1, Ordering::SeqCst);

    // — ShadePacket: writer waits via futex until all readers drain out
    loop {
        // Try to acquire when unlocked (state == 0)
        if (*rwlock)
            .state
            .compare_exchange_weak(0, WRITE_LOCKED, Ordering::AcqRel, Ordering::Relaxed)
            .is_ok()
        {
            (*rwlock).waiting_writers.fetch_sub(1, Ordering::SeqCst);
            (*rwlock).writer.store(self_id, Ordering::Release);
            return ESUCCESS;
        }

        let state = (*rwlock).state.load(Ordering::Acquire);
        futex_wait((*rwlock).state.as_ptr(), state);
    }
}

/// Try to acquire write lock
#[no_mangle]
pub unsafe extern "C" fn pthread_rwlock_trywrlock(rwlock: *mut pthread_rwlock_t) -> c_int {
    if rwlock.is_null() {
        return EINVAL;
    }

    let self_id = pthread_self();

    // Try to acquire when unlocked
    if (*rwlock)
        .state
        .compare_exchange(0, WRITE_LOCKED, Ordering::AcqRel, Ordering::Relaxed)
        .is_ok()
    {
        (*rwlock).writer.store(self_id, Ordering::Release);
        ESUCCESS
    } else {
        EBUSY
    }
}

/// Release lock (read or write)
#[no_mangle]
pub unsafe extern "C" fn pthread_rwlock_unlock(rwlock: *mut pthread_rwlock_t) -> c_int {
    if rwlock.is_null() {
        return EINVAL;
    }

    let state = (*rwlock).state.load(Ordering::Acquire);

    if (state & WRITE_LOCKED) != 0 {
        // Release write lock
        (*rwlock).writer.store(0, Ordering::Release);
        (*rwlock).state.store(0, Ordering::Release);
        // — ShadePacket: writer done, wake all waiting readers and writers
        futex_wake((*rwlock).state.as_ptr(), i32::MAX as u32);
    } else if (state & READER_MASK) > 0 {
        // Release read lock (decrement reader count)
        let prev = (*rwlock).state.fetch_sub(1, Ordering::AcqRel);
        // — ShadePacket: if we were the last reader, wake a waiting writer
        if (prev & READER_MASK) == 1 {
            futex_wake((*rwlock).state.as_ptr(), i32::MAX as u32);
        }
    } else {
        // Not locked
        return EINVAL;
    }

    ESUCCESS
}

/// Initialize rwlock attributes
#[no_mangle]
pub unsafe extern "C" fn pthread_rwlockattr_init(attr: *mut pthread_rwlockattr_t) -> c_int {
    if attr.is_null() {
        return EINVAL;
    }
    *attr = pthread_rwlockattr_t::new();
    ESUCCESS
}

/// Destroy rwlock attributes
#[no_mangle]
pub unsafe extern "C" fn pthread_rwlockattr_destroy(attr: *mut pthread_rwlockattr_t) -> c_int {
    if attr.is_null() {
        return EINVAL;
    }
    ESUCCESS
}
