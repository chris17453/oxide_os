//! One-time initialization
//! — WireSaint: once means ONCE, and now we actually sleep instead of burning cores

use core::ffi::c_int;
use core::sync::atomic::{AtomicU32, Ordering};

/// Futex wait — blocks if *addr == expected. No timeout.
/// — WireSaint: park until the initializer finishes their one job
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
/// — WireSaint: init's done, wake the patient masses
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
                // — WireSaint: init complete, wake all the threads waiting on this once
                futex_wake((*once_control).state.as_ptr(), i32::MAX as u32);
                return ESUCCESS;
            }
            Err(ONCE_DONE) => {
                // Already done
                return ESUCCESS;
            }
            Err(ONCE_RUNNING) => {
                // — WireSaint: someone else is initializing, futex-sleep on ONCE_RUNNING
                while (*once_control).state.load(Ordering::Acquire) == ONCE_RUNNING {
                    futex_wait((*once_control).state.as_ptr(), ONCE_RUNNING);
                }
                return ESUCCESS;
            }
            Err(_) => {
                // Spurious CAS failure, retry the loop
                core::hint::spin_loop();
            }
        }
    }
}
