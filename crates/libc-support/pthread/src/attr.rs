//! Thread attributes

use core::ffi::c_int;

use crate::{EINVAL, ESUCCESS, PTHREAD_CREATE_DETACHED, PTHREAD_CREATE_JOINABLE};

/// Default stack size (2MB)
const DEFAULT_STACK_SIZE: usize = 2 * 1024 * 1024;

/// Minimum stack size
const PTHREAD_STACK_MIN: usize = 16384;

/// Thread attributes structure
#[repr(C)]
pub struct pthread_attr_t {
    /// Detach state
    pub detachstate: c_int,
    /// Stack size
    pub stacksize: usize,
    /// Guard size
    pub guardsize: usize,
    /// Stack address (if set)
    pub stackaddr: *mut u8,
    /// Scheduling policy
    pub schedpolicy: c_int,
    /// Scheduling priority
    pub schedparam_priority: c_int,
    /// Inherit scheduler
    pub inheritsched: c_int,
    /// Scope
    pub scope: c_int,
}

// Raw pointer, but we manage access
unsafe impl Send for pthread_attr_t {}
unsafe impl Sync for pthread_attr_t {}

impl pthread_attr_t {
    pub const fn new() -> Self {
        Self {
            detachstate: PTHREAD_CREATE_JOINABLE,
            stacksize: DEFAULT_STACK_SIZE,
            guardsize: 4096,
            stackaddr: core::ptr::null_mut(),
            schedpolicy: 0,
            schedparam_priority: 0,
            inheritsched: 0,
            scope: 0,
        }
    }
}

/// Initialize thread attributes
#[no_mangle]
pub unsafe extern "C" fn pthread_attr_init(attr: *mut pthread_attr_t) -> c_int {
    if attr.is_null() {
        return EINVAL;
    }
    *attr = pthread_attr_t::new();
    ESUCCESS
}

/// Destroy thread attributes
#[no_mangle]
pub unsafe extern "C" fn pthread_attr_destroy(attr: *mut pthread_attr_t) -> c_int {
    if attr.is_null() {
        return EINVAL;
    }
    ESUCCESS
}

/// Set detach state
#[no_mangle]
pub unsafe extern "C" fn pthread_attr_setdetachstate(
    attr: *mut pthread_attr_t,
    detachstate: c_int,
) -> c_int {
    if attr.is_null() {
        return EINVAL;
    }
    match detachstate {
        PTHREAD_CREATE_JOINABLE | PTHREAD_CREATE_DETACHED => {
            (*attr).detachstate = detachstate;
            ESUCCESS
        }
        _ => EINVAL,
    }
}

/// Get detach state
#[no_mangle]
pub unsafe extern "C" fn pthread_attr_getdetachstate(
    attr: *const pthread_attr_t,
    detachstate: *mut c_int,
) -> c_int {
    if attr.is_null() || detachstate.is_null() {
        return EINVAL;
    }
    *detachstate = (*attr).detachstate;
    ESUCCESS
}

/// Set stack size
#[no_mangle]
pub unsafe extern "C" fn pthread_attr_setstacksize(
    attr: *mut pthread_attr_t,
    stacksize: usize,
) -> c_int {
    if attr.is_null() {
        return EINVAL;
    }
    if stacksize < PTHREAD_STACK_MIN {
        return EINVAL;
    }
    (*attr).stacksize = stacksize;
    ESUCCESS
}

/// Get stack size
#[no_mangle]
pub unsafe extern "C" fn pthread_attr_getstacksize(
    attr: *const pthread_attr_t,
    stacksize: *mut usize,
) -> c_int {
    if attr.is_null() || stacksize.is_null() {
        return EINVAL;
    }
    *stacksize = (*attr).stacksize;
    ESUCCESS
}

/// Set guard size
#[no_mangle]
pub unsafe extern "C" fn pthread_attr_setguardsize(
    attr: *mut pthread_attr_t,
    guardsize: usize,
) -> c_int {
    if attr.is_null() {
        return EINVAL;
    }
    (*attr).guardsize = guardsize;
    ESUCCESS
}

/// Get guard size
#[no_mangle]
pub unsafe extern "C" fn pthread_attr_getguardsize(
    attr: *const pthread_attr_t,
    guardsize: *mut usize,
) -> c_int {
    if attr.is_null() || guardsize.is_null() {
        return EINVAL;
    }
    *guardsize = (*attr).guardsize;
    ESUCCESS
}

/// Set stack
#[no_mangle]
pub unsafe extern "C" fn pthread_attr_setstack(
    attr: *mut pthread_attr_t,
    stackaddr: *mut u8,
    stacksize: usize,
) -> c_int {
    if attr.is_null() {
        return EINVAL;
    }
    if stacksize < PTHREAD_STACK_MIN {
        return EINVAL;
    }
    (*attr).stackaddr = stackaddr;
    (*attr).stacksize = stacksize;
    ESUCCESS
}

/// Get stack
#[no_mangle]
pub unsafe extern "C" fn pthread_attr_getstack(
    attr: *const pthread_attr_t,
    stackaddr: *mut *mut u8,
    stacksize: *mut usize,
) -> c_int {
    if attr.is_null() || stackaddr.is_null() || stacksize.is_null() {
        return EINVAL;
    }
    *stackaddr = (*attr).stackaddr;
    *stacksize = (*attr).stacksize;
    ESUCCESS
}

/// Set scheduling policy
#[no_mangle]
pub unsafe extern "C" fn pthread_attr_setschedpolicy(
    attr: *mut pthread_attr_t,
    policy: c_int,
) -> c_int {
    if attr.is_null() {
        return EINVAL;
    }
    (*attr).schedpolicy = policy;
    ESUCCESS
}

/// Get scheduling policy
#[no_mangle]
pub unsafe extern "C" fn pthread_attr_getschedpolicy(
    attr: *const pthread_attr_t,
    policy: *mut c_int,
) -> c_int {
    if attr.is_null() || policy.is_null() {
        return EINVAL;
    }
    *policy = (*attr).schedpolicy;
    ESUCCESS
}
