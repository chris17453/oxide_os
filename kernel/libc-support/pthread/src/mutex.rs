//! Mutex implementation

use core::ffi::c_int;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use crate::{
    pthread_self, EBUSY, EDEADLK, EINVAL, ESUCCESS, PTHREAD_MUTEX_ERRORCHECK, PTHREAD_MUTEX_NORMAL,
    PTHREAD_MUTEX_RECURSIVE,
};

/// Mutex state
const MUTEX_UNLOCKED: u32 = 0;
const MUTEX_LOCKED: u32 = 1;

/// Mutex structure
#[repr(C)]
pub struct pthread_mutex_t {
    /// Lock state
    state: AtomicU32,
    /// Mutex type
    kind: AtomicU32,
    /// Owner thread (for recursive/errorcheck)
    owner: AtomicU64,
    /// Recursion count (for recursive mutex)
    count: AtomicU32,
}

impl pthread_mutex_t {
    /// Static initializer for normal mutex
    pub const INITIALIZER: Self = Self {
        state: AtomicU32::new(MUTEX_UNLOCKED),
        kind: AtomicU32::new(PTHREAD_MUTEX_NORMAL as u32),
        owner: AtomicU64::new(0),
        count: AtomicU32::new(0),
    };
}

/// Mutex attributes
#[repr(C)]
pub struct pthread_mutexattr_t {
    /// Mutex type
    pub kind: c_int,
    /// Process sharing
    pub pshared: c_int,
}

impl pthread_mutexattr_t {
    pub const fn new() -> Self {
        Self {
            kind: PTHREAD_MUTEX_NORMAL,
            pshared: 0,
        }
    }
}

/// Initialize mutex
#[no_mangle]
pub unsafe extern "C" fn pthread_mutex_init(
    mutex: *mut pthread_mutex_t,
    attr: *const pthread_mutexattr_t,
) -> c_int {
    if mutex.is_null() {
        return EINVAL;
    }

    let kind = if !attr.is_null() {
        (*attr).kind
    } else {
        PTHREAD_MUTEX_NORMAL
    };

    (*mutex).state.store(MUTEX_UNLOCKED, Ordering::SeqCst);
    (*mutex).kind.store(kind as u32, Ordering::SeqCst);
    (*mutex).owner.store(0, Ordering::SeqCst);
    (*mutex).count.store(0, Ordering::SeqCst);

    ESUCCESS
}

/// Destroy mutex
#[no_mangle]
pub unsafe extern "C" fn pthread_mutex_destroy(mutex: *mut pthread_mutex_t) -> c_int {
    if mutex.is_null() {
        return EINVAL;
    }

    // Check if locked
    if (*mutex).state.load(Ordering::SeqCst) != MUTEX_UNLOCKED {
        return EBUSY;
    }

    ESUCCESS
}

/// Lock mutex
#[no_mangle]
pub unsafe extern "C" fn pthread_mutex_lock(mutex: *mut pthread_mutex_t) -> c_int {
    if mutex.is_null() {
        return EINVAL;
    }

    let kind = (*mutex).kind.load(Ordering::SeqCst) as c_int;
    let self_id = pthread_self();

    match kind {
        PTHREAD_MUTEX_NORMAL => {
            // Simple spinlock (would use futex in real implementation)
            while (*mutex)
                .state
                .compare_exchange_weak(
                    MUTEX_UNLOCKED,
                    MUTEX_LOCKED,
                    Ordering::Acquire,
                    Ordering::Relaxed,
                )
                .is_err()
            {
                core::hint::spin_loop();
            }
            (*mutex).owner.store(self_id, Ordering::Release);
        }
        PTHREAD_MUTEX_RECURSIVE => {
            let owner = (*mutex).owner.load(Ordering::Acquire);
            if owner == self_id {
                // Already own it, increment count
                (*mutex).count.fetch_add(1, Ordering::SeqCst);
                return ESUCCESS;
            }

            // Try to acquire
            while (*mutex)
                .state
                .compare_exchange_weak(
                    MUTEX_UNLOCKED,
                    MUTEX_LOCKED,
                    Ordering::Acquire,
                    Ordering::Relaxed,
                )
                .is_err()
            {
                core::hint::spin_loop();
            }
            (*mutex).owner.store(self_id, Ordering::Release);
            (*mutex).count.store(1, Ordering::Release);
        }
        PTHREAD_MUTEX_ERRORCHECK => {
            let owner = (*mutex).owner.load(Ordering::Acquire);
            if owner == self_id {
                return EDEADLK;
            }

            while (*mutex)
                .state
                .compare_exchange_weak(
                    MUTEX_UNLOCKED,
                    MUTEX_LOCKED,
                    Ordering::Acquire,
                    Ordering::Relaxed,
                )
                .is_err()
            {
                core::hint::spin_loop();
            }
            (*mutex).owner.store(self_id, Ordering::Release);
        }
        _ => return EINVAL,
    }

    ESUCCESS
}

/// Try to lock mutex
#[no_mangle]
pub unsafe extern "C" fn pthread_mutex_trylock(mutex: *mut pthread_mutex_t) -> c_int {
    if mutex.is_null() {
        return EINVAL;
    }

    let kind = (*mutex).kind.load(Ordering::SeqCst) as c_int;
    let self_id = pthread_self();

    match kind {
        PTHREAD_MUTEX_NORMAL => {
            if (*mutex)
                .state
                .compare_exchange(
                    MUTEX_UNLOCKED,
                    MUTEX_LOCKED,
                    Ordering::Acquire,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                (*mutex).owner.store(self_id, Ordering::Release);
                ESUCCESS
            } else {
                EBUSY
            }
        }
        PTHREAD_MUTEX_RECURSIVE => {
            let owner = (*mutex).owner.load(Ordering::Acquire);
            if owner == self_id {
                (*mutex).count.fetch_add(1, Ordering::SeqCst);
                return ESUCCESS;
            }

            if (*mutex)
                .state
                .compare_exchange(
                    MUTEX_UNLOCKED,
                    MUTEX_LOCKED,
                    Ordering::Acquire,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                (*mutex).owner.store(self_id, Ordering::Release);
                (*mutex).count.store(1, Ordering::Release);
                ESUCCESS
            } else {
                EBUSY
            }
        }
        PTHREAD_MUTEX_ERRORCHECK => {
            let owner = (*mutex).owner.load(Ordering::Acquire);
            if owner == self_id {
                return EDEADLK;
            }

            if (*mutex)
                .state
                .compare_exchange(
                    MUTEX_UNLOCKED,
                    MUTEX_LOCKED,
                    Ordering::Acquire,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                (*mutex).owner.store(self_id, Ordering::Release);
                ESUCCESS
            } else {
                EBUSY
            }
        }
        _ => EINVAL,
    }
}

/// Unlock mutex
#[no_mangle]
pub unsafe extern "C" fn pthread_mutex_unlock(mutex: *mut pthread_mutex_t) -> c_int {
    if mutex.is_null() {
        return EINVAL;
    }

    let kind = (*mutex).kind.load(Ordering::SeqCst) as c_int;

    match kind {
        PTHREAD_MUTEX_NORMAL => {
            (*mutex).owner.store(0, Ordering::Release);
            (*mutex).state.store(MUTEX_UNLOCKED, Ordering::Release);
        }
        PTHREAD_MUTEX_RECURSIVE => {
            let count = (*mutex).count.fetch_sub(1, Ordering::SeqCst);
            if count == 1 {
                (*mutex).owner.store(0, Ordering::Release);
                (*mutex).state.store(MUTEX_UNLOCKED, Ordering::Release);
            }
        }
        PTHREAD_MUTEX_ERRORCHECK => {
            let self_id = pthread_self();
            let owner = (*mutex).owner.load(Ordering::Acquire);
            if owner != self_id {
                return EINVAL;
            }
            (*mutex).owner.store(0, Ordering::Release);
            (*mutex).state.store(MUTEX_UNLOCKED, Ordering::Release);
        }
        _ => return EINVAL,
    }

    // In real implementation: futex wake

    ESUCCESS
}

/// Initialize mutex attributes
#[no_mangle]
pub unsafe extern "C" fn pthread_mutexattr_init(attr: *mut pthread_mutexattr_t) -> c_int {
    if attr.is_null() {
        return EINVAL;
    }
    *attr = pthread_mutexattr_t::new();
    ESUCCESS
}

/// Destroy mutex attributes
#[no_mangle]
pub unsafe extern "C" fn pthread_mutexattr_destroy(attr: *mut pthread_mutexattr_t) -> c_int {
    if attr.is_null() {
        return EINVAL;
    }
    ESUCCESS
}

/// Set mutex type
#[no_mangle]
pub unsafe extern "C" fn pthread_mutexattr_settype(
    attr: *mut pthread_mutexattr_t,
    kind: c_int,
) -> c_int {
    if attr.is_null() {
        return EINVAL;
    }
    match kind {
        PTHREAD_MUTEX_NORMAL | PTHREAD_MUTEX_RECURSIVE | PTHREAD_MUTEX_ERRORCHECK => {
            (*attr).kind = kind;
            ESUCCESS
        }
        _ => EINVAL,
    }
}

/// Get mutex type
#[no_mangle]
pub unsafe extern "C" fn pthread_mutexattr_gettype(
    attr: *const pthread_mutexattr_t,
    kind: *mut c_int,
) -> c_int {
    if attr.is_null() || kind.is_null() {
        return EINVAL;
    }
    *kind = (*attr).kind;
    ESUCCESS
}
