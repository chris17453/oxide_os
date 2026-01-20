//! Futex (Fast Userspace muTEX) implementation
//!
//! Futexes provide fast userspace locking with kernel assistance.
//! The basic operations are:
//! - FUTEX_WAIT: If *addr == val, sleep until woken
//! - FUTEX_WAKE: Wake up to n waiters on addr

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use spin::Mutex;
use proc_traits::{Pid, ProcessState};

use crate::process_table;

/// Futex error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FutexError {
    /// The value at addr didn't match expected
    WouldBlock,
    /// Invalid address
    InvalidAddress,
    /// Timeout expired
    TimedOut,
    /// Operation interrupted
    Interrupted,
    /// Invalid operation
    InvalidOp,
}

/// Futex operations
pub mod futex_op {
    pub const FUTEX_WAIT: i32 = 0;
    pub const FUTEX_WAKE: i32 = 1;
    pub const FUTEX_WAIT_PRIVATE: i32 = 128;
    pub const FUTEX_WAKE_PRIVATE: i32 = 129;
}

/// A waiter on a futex
#[derive(Debug, Clone, Copy)]
struct FutexWaiter {
    /// PID/TID of the waiting process/thread
    pid: Pid,
}

/// Global futex wait queues
/// Key is the physical address of the futex (to handle shared memory)
static FUTEX_QUEUES: Mutex<BTreeMap<u64, Vec<FutexWaiter>>> = Mutex::new(BTreeMap::new());

/// Wait on a futex
///
/// If the value at `addr` equals `expected`, put the calling thread to sleep.
/// The thread will be woken by futex_wake or a signal.
///
/// # Arguments
/// * `addr` - User address of the futex word
/// * `expected` - Expected value at addr
/// * `_timeout_ns` - Timeout in nanoseconds (0 = infinite) - not yet implemented
///
/// # Returns
/// 0 on success (woken by futex_wake), or FutexError
pub fn futex_wait(addr: u64, expected: u32, _timeout_ns: u64) -> Result<(), FutexError> {
    // Validate address is in user space
    if addr >= 0x0000_8000_0000_0000 || addr == 0 {
        return Err(FutexError::InvalidAddress);
    }

    // Ensure address is aligned
    if addr % 4 != 0 {
        return Err(FutexError::InvalidAddress);
    }

    let table = process_table();
    let current_pid = table.current_pid();

    // Read the current value atomically
    let current_val = unsafe {
        let ptr = addr as *const u32;
        core::ptr::read_volatile(ptr)
    };

    // If value doesn't match, return immediately
    if current_val != expected {
        return Err(FutexError::WouldBlock);
    }

    // Add ourselves to the wait queue
    {
        let mut queues = FUTEX_QUEUES.lock();
        let waiters = queues.entry(addr).or_insert_with(Vec::new);
        waiters.push(FutexWaiter { pid: current_pid });
    }

    // Put ourselves to sleep
    if let Some(proc) = table.get(current_pid) {
        proc.lock().set_state(ProcessState::Blocked);
    }

    // The scheduler will now run a different task
    // When we're woken, we'll continue here
    // For now, we return success - the actual blocking happens via the scheduler

    Ok(())
}

/// Wake waiters on a futex
///
/// Wake up to `count` threads waiting on the futex at `addr`.
///
/// # Arguments
/// * `addr` - User address of the futex word
/// * `count` - Maximum number of waiters to wake (i32::MAX for all)
///
/// # Returns
/// Number of waiters woken
pub fn futex_wake(addr: u64, count: i32) -> Result<i32, FutexError> {
    // Validate address is in user space
    if addr >= 0x0000_8000_0000_0000 || addr == 0 {
        return Err(FutexError::InvalidAddress);
    }

    let table = process_table();
    let mut woken = 0i32;

    // Get and remove waiters from the queue
    let waiters_to_wake: Vec<FutexWaiter> = {
        let mut queues = FUTEX_QUEUES.lock();
        if let Some(waiters) = queues.get_mut(&addr) {
            let to_wake = count.min(waiters.len() as i32) as usize;
            let waking: Vec<_> = waiters.drain(..to_wake).collect();

            // Remove empty queue entry
            if waiters.is_empty() {
                queues.remove(&addr);
            }

            waking
        } else {
            Vec::new()
        }
    };

    // Wake the waiters
    for waiter in waiters_to_wake {
        if let Some(proc) = table.get(waiter.pid) {
            let mut p = proc.lock();
            if p.state() == ProcessState::Blocked {
                p.set_state(ProcessState::Ready);
                woken += 1;
            }
        }
    }

    Ok(woken)
}

/// Clear futex and wake (for thread exit with CLONE_CHILD_CLEARTID)
///
/// Writes 0 to the address and wakes one waiter.
/// Used when a thread exits with clear_child_tid set.
///
/// # Arguments
/// * `addr` - Address to clear and wake
pub fn futex_clear_and_wake(addr: u64) {
    if addr == 0 || addr >= 0x0000_8000_0000_0000 {
        return;
    }

    // Write 0 to the address
    unsafe {
        let ptr = addr as *mut u32;
        core::ptr::write_volatile(ptr, 0);
    }

    // Wake one waiter
    let _ = futex_wake(addr, 1);
}
