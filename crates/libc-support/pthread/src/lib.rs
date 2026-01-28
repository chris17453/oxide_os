//! POSIX Threads (pthreads) Implementation
//!
//! Provides pthread API for self-hosting support.

#![no_std]
#![allow(unused)]
#![allow(unsafe_op_in_unsafe_fn)]
#![allow(non_camel_case_types)]
#![allow(unsafe_attr_outside_unsafe)]

extern crate alloc;

// When building as staticlib, we need panic handler and allocator stubs
// These will be overridden by the actual program's implementations
#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

// Stub allocator - will be overridden by program's allocator
#[cfg(not(test))]
use core::alloc::{GlobalAlloc, Layout};

#[cfg(not(test))]
struct StubAllocator;

#[cfg(not(test))]
unsafe impl GlobalAlloc for StubAllocator {
    unsafe fn alloc(&self, _layout: Layout) -> *mut u8 {
        core::ptr::null_mut()
    }
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

#[cfg(not(test))]
#[global_allocator]
static ALLOCATOR: StubAllocator = StubAllocator;

pub mod attr;
pub mod barrier;
pub mod condvar;
pub mod mutex;
pub mod once;
pub mod rwlock;
pub mod thread;
pub mod tls;

use core::ffi::{c_int, c_void};

pub use attr::{
    pthread_attr_destroy, pthread_attr_getdetachstate, pthread_attr_getstacksize,
    pthread_attr_init, pthread_attr_setdetachstate, pthread_attr_setstacksize, pthread_attr_t,
};
pub use barrier::{
    pthread_barrier_destroy, pthread_barrier_init, pthread_barrier_t, pthread_barrier_wait,
    pthread_barrierattr_t, PTHREAD_BARRIER_SERIAL_THREAD,
};
pub use condvar::{
    pthread_cond_broadcast, pthread_cond_destroy, pthread_cond_init, pthread_cond_signal,
    pthread_cond_t, pthread_cond_timedwait, pthread_cond_wait, pthread_condattr_t,
};
pub use mutex::{
    pthread_mutex_destroy, pthread_mutex_init, pthread_mutex_lock, pthread_mutex_t,
    pthread_mutex_trylock, pthread_mutex_unlock, pthread_mutexattr_t,
};
pub use once::{pthread_once, pthread_once_t, PTHREAD_ONCE_INIT};
pub use rwlock::{
    pthread_rwlock_destroy, pthread_rwlock_init, pthread_rwlock_rdlock, pthread_rwlock_t,
    pthread_rwlock_tryrdlock, pthread_rwlock_trywrlock, pthread_rwlock_unlock,
    pthread_rwlock_wrlock, pthread_rwlockattr_t,
};
pub use thread::{
    pthread_create, pthread_detach, pthread_exit, pthread_join, pthread_self, pthread_t,
};
pub use tls::{
    pthread_getspecific, pthread_key_create, pthread_key_delete, pthread_key_t, pthread_setspecific,
};

/// Error codes
pub const ESUCCESS: c_int = 0;
pub const EINVAL: c_int = 22;
pub const ENOMEM: c_int = 12;
pub const EAGAIN: c_int = 11;
pub const EBUSY: c_int = 16;
pub const EDEADLK: c_int = 35;
pub const EPERM: c_int = 1;
pub const ETIMEDOUT: c_int = 110;

/// Detach state
pub const PTHREAD_CREATE_JOINABLE: c_int = 0;
pub const PTHREAD_CREATE_DETACHED: c_int = 1;

/// Mutex types
pub const PTHREAD_MUTEX_NORMAL: c_int = 0;
pub const PTHREAD_MUTEX_RECURSIVE: c_int = 1;
pub const PTHREAD_MUTEX_ERRORCHECK: c_int = 2;
pub const PTHREAD_MUTEX_DEFAULT: c_int = PTHREAD_MUTEX_NORMAL;

/// Process sharing
pub const PTHREAD_PROCESS_PRIVATE: c_int = 0;
pub const PTHREAD_PROCESS_SHARED: c_int = 1;

/// Thread cancellation
pub const PTHREAD_CANCEL_ENABLE: c_int = 0;
pub const PTHREAD_CANCEL_DISABLE: c_int = 1;
pub const PTHREAD_CANCEL_DEFERRED: c_int = 0;
pub const PTHREAD_CANCEL_ASYNCHRONOUS: c_int = 1;
pub const PTHREAD_CANCELED: *mut c_void = -1isize as *mut c_void;
