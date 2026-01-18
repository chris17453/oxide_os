//! One-time initialization

use core::ffi::c_int;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::ESUCCESS;

/// Once state values
const ONCE_INIT: u32 = 0;
const ONCE_RUNNING: u32 = 1;
const ONCE_DONE: u32 = 2;

/// Once control structure
#[repr(C)]
pub struct pthread_once_t {
    state: AtomicU32,
}

/// Static initializer
pub const PTHREAD_ONCE_INIT: pthread_once_t = pthread_once_t {
    state: AtomicU32::new(ONCE_INIT),
};

impl pthread_once_t {
    pub const fn new() -> Self {
        Self {
            state: AtomicU32::new(ONCE_INIT),
        }
    }
}

/// Execute init_routine exactly once
#[no_mangle]
pub unsafe extern "C" fn pthread_once(
    once_control: *mut pthread_once_t,
    init_routine: extern "C" fn(),
) -> c_int {
    if once_control.is_null() {
        return ESUCCESS; // Be lenient
    }

    // Fast path: already done
    if (*once_control).state.load(Ordering::Acquire) == ONCE_DONE {
        return ESUCCESS;
    }

    // Try to be the one to run the init
    loop {
        match (*once_control).state.compare_exchange_weak(
            ONCE_INIT,
            ONCE_RUNNING,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => {
                // We won - run the init
                init_routine();
                (*once_control).state.store(ONCE_DONE, Ordering::Release);
                // In real implementation: futex_wake all
                return ESUCCESS;
            }
            Err(ONCE_DONE) => {
                // Already done
                return ESUCCESS;
            }
            Err(ONCE_RUNNING) => {
                // Someone else is running it - wait
                // In real implementation: futex_wait
                while (*once_control).state.load(Ordering::Acquire) == ONCE_RUNNING {
                    core::hint::spin_loop();
                }
                return ESUCCESS;
            }
            Err(_) => {
                // Spurious failure, retry
                core::hint::spin_loop();
            }
        }
    }
}
