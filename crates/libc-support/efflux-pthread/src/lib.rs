//! POSIX Threads (pthreads) Implementation
//!
//! Provides pthread API for self-hosting support.

#![no_std]
#![allow(unsafe_op_in_unsafe_fn)]
#![allow(non_camel_case_types)]
#![allow(unused_imports)]
#![allow(unsafe_attr_outside_unsafe)]

extern crate alloc;

pub mod thread;
pub mod mutex;
pub mod condvar;
pub mod rwlock;
pub mod barrier;
pub mod tls;
pub mod once;
pub mod attr;

use core::ffi::{c_int, c_void};

pub use thread::{pthread_t, pthread_create, pthread_join, pthread_detach, pthread_exit, pthread_self};
pub use mutex::{pthread_mutex_t, pthread_mutexattr_t, pthread_mutex_init, pthread_mutex_destroy,
                pthread_mutex_lock, pthread_mutex_trylock, pthread_mutex_unlock};
pub use condvar::{pthread_cond_t, pthread_condattr_t, pthread_cond_init, pthread_cond_destroy,
                  pthread_cond_wait, pthread_cond_timedwait, pthread_cond_signal, pthread_cond_broadcast};
pub use rwlock::{pthread_rwlock_t, pthread_rwlockattr_t, pthread_rwlock_init, pthread_rwlock_destroy,
                 pthread_rwlock_rdlock, pthread_rwlock_wrlock, pthread_rwlock_unlock,
                 pthread_rwlock_tryrdlock, pthread_rwlock_trywrlock};
pub use barrier::{pthread_barrier_t, pthread_barrierattr_t, pthread_barrier_init, pthread_barrier_destroy,
                  pthread_barrier_wait, PTHREAD_BARRIER_SERIAL_THREAD};
pub use tls::{pthread_key_t, pthread_key_create, pthread_key_delete, pthread_getspecific, pthread_setspecific};
pub use once::{pthread_once_t, pthread_once, PTHREAD_ONCE_INIT};
pub use attr::{pthread_attr_t, pthread_attr_init, pthread_attr_destroy, pthread_attr_setdetachstate,
               pthread_attr_getdetachstate, pthread_attr_setstacksize, pthread_attr_getstacksize};

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
