//! Barrier implementation

#![allow(non_camel_case_types)]

use core::ffi::c_int;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::{EINVAL, ESUCCESS};

/// Return value for one thread at barrier
pub const PTHREAD_BARRIER_SERIAL_THREAD: c_int = -1;

/// Barrier structure
#[repr(C)]
pub struct pthread_barrier_t {
    /// Number of threads required
    count: AtomicU32,
    /// Current number of waiting threads
    waiting: AtomicU32,
    /// Generation (for reuse)
    generation: AtomicU32,
}

impl pthread_barrier_t {
    pub const fn new() -> Self {
        Self {
            count: AtomicU32::new(0),
            waiting: AtomicU32::new(0),
            generation: AtomicU32::new(0),
        }
    }
}

/// Barrier attributes
#[repr(C)]
pub struct pthread_barrierattr_t {
    /// Process sharing
    pub pshared: c_int,
}

impl pthread_barrierattr_t {
    pub const fn new() -> Self {
        Self { pshared: 0 }
    }
}

/// Initialize barrier
#[no_mangle]
pub unsafe extern "C" fn pthread_barrier_init(
    barrier: *mut pthread_barrier_t,
    _attr: *const pthread_barrierattr_t,
    count: u32,
) -> c_int {
    if barrier.is_null() || count == 0 {
        return EINVAL;
    }

    (*barrier).count.store(count, Ordering::SeqCst);
    (*barrier).waiting.store(0, Ordering::SeqCst);
    (*barrier).generation.store(0, Ordering::SeqCst);

    ESUCCESS
}

/// Destroy barrier
#[no_mangle]
pub unsafe extern "C" fn pthread_barrier_destroy(barrier: *mut pthread_barrier_t) -> c_int {
    if barrier.is_null() {
        return EINVAL;
    }

    // Check if threads are waiting
    if (*barrier).waiting.load(Ordering::SeqCst) > 0 {
        return EINVAL;
    }

    ESUCCESS
}

/// Wait at barrier
#[no_mangle]
pub unsafe extern "C" fn pthread_barrier_wait(barrier: *mut pthread_barrier_t) -> c_int {
    if barrier.is_null() {
        return EINVAL;
    }

    let count = (*barrier).count.load(Ordering::SeqCst);
    let generation = (*barrier).generation.load(Ordering::Acquire);

    // Increment waiting count
    let waiting = (*barrier).waiting.fetch_add(1, Ordering::SeqCst) + 1;

    if waiting == count {
        // Last thread to arrive - release all
        (*barrier).waiting.store(0, Ordering::SeqCst);
        (*barrier).generation.fetch_add(1, Ordering::Release);
        // In real implementation: futex_wake all
        return PTHREAD_BARRIER_SERIAL_THREAD;
    }

    // Wait for generation to change
    // In real implementation: futex_wait
    while (*barrier).generation.load(Ordering::Acquire) == generation {
        core::hint::spin_loop();
    }

    ESUCCESS
}

/// Initialize barrier attributes
#[no_mangle]
pub unsafe extern "C" fn pthread_barrierattr_init(attr: *mut pthread_barrierattr_t) -> c_int {
    if attr.is_null() {
        return EINVAL;
    }
    *attr = pthread_barrierattr_t::new();
    ESUCCESS
}

/// Destroy barrier attributes
#[no_mangle]
pub unsafe extern "C" fn pthread_barrierattr_destroy(attr: *mut pthread_barrierattr_t) -> c_int {
    if attr.is_null() {
        return EINVAL;
    }
    ESUCCESS
}
