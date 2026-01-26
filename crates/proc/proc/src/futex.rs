//! Futex (Fast Userspace muTEX) implementation
//!
//! Futexes provide fast userspace locking with kernel assistance.
//! The basic operations are:
//! - FUTEX_WAIT: If *addr == val, sleep until woken
//! - FUTEX_WAKE: Wake up to n waiters on addr
//!
//! This module manages the wait queues and returns actions for the
//! kernel/scheduler to execute. The actual blocking/waking of processes
//! is done by the kernel which has scheduler access.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use proc_traits::Pid;
use spin::Mutex;

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

/// Result of futex_wait_prepare - tells kernel what to do
#[derive(Debug)]
pub enum FutexWaitResult {
    /// Value didn't match - don't block, return EAGAIN
    ValueMismatch,
    /// Should block - PID has been added to wait queue
    ShouldBlock,
}

/// Prepare for futex wait
///
/// Checks if the value at `addr` equals `expected`, and if so,
/// adds the calling thread to the wait queue.
///
/// # Arguments
/// * `current_pid` - PID of the calling process (from scheduler)
/// * `addr` - User address of the futex word
/// * `expected` - Expected value at addr
///
/// # Returns
/// FutexWaitResult indicating whether caller should block
pub fn futex_wait_prepare(
    current_pid: Pid,
    addr: u64,
    expected: u32,
) -> Result<FutexWaitResult, FutexError> {
    // Validate address is in user space
    if addr >= 0x0000_8000_0000_0000 || addr == 0 {
        return Err(FutexError::InvalidAddress);
    }

    // Ensure address is aligned
    if addr % 4 != 0 {
        return Err(FutexError::InvalidAddress);
    }

    // Read the current value atomically
    let current_val = unsafe {
        let ptr = addr as *const u32;
        core::ptr::read_volatile(ptr)
    };

    // If value doesn't match, return immediately
    if current_val != expected {
        return Ok(FutexWaitResult::ValueMismatch);
    }

    // Add ourselves to the wait queue
    {
        let mut queues = FUTEX_QUEUES.lock();
        let waiters = queues.entry(addr).or_insert_with(Vec::new);
        waiters.push(FutexWaiter { pid: current_pid });
    }

    // Caller should block via scheduler
    Ok(FutexWaitResult::ShouldBlock)
}

/// Remove a waiter from the futex queue (e.g., on timeout or signal)
///
/// Called when a blocked process is woken by something other than futex_wake
/// (like a signal or timeout).
pub fn futex_wait_cancel(pid: Pid, addr: u64) {
    let mut queues = FUTEX_QUEUES.lock();
    if let Some(waiters) = queues.get_mut(&addr) {
        waiters.retain(|w| w.pid != pid);
        if waiters.is_empty() {
            queues.remove(&addr);
        }
    }
}

/// Wake waiters on a futex
///
/// Wake up to `count` threads waiting on the futex at `addr`.
/// Returns the list of PIDs to wake.
///
/// # Arguments
/// * `addr` - User address of the futex word
/// * `count` - Maximum number of waiters to wake (i32::MAX for all)
///
/// # Returns
/// Vector of PIDs to wake via scheduler
pub fn futex_wake(addr: u64, count: i32) -> Result<Vec<Pid>, FutexError> {
    // Validate address is in user space
    if addr >= 0x0000_8000_0000_0000 || addr == 0 {
        return Err(FutexError::InvalidAddress);
    }

    // Get and remove waiters from the queue
    let waiters_to_wake: Vec<Pid> = {
        let mut queues = FUTEX_QUEUES.lock();
        if let Some(waiters) = queues.get_mut(&addr) {
            let to_wake = count.min(waiters.len() as i32) as usize;
            let waking: Vec<Pid> = waiters.drain(..to_wake).map(|w| w.pid).collect();

            // Remove empty queue entry
            if waiters.is_empty() {
                queues.remove(&addr);
            }

            waking
        } else {
            Vec::new()
        }
    };

    Ok(waiters_to_wake)
}

/// Clear futex and wake (for thread exit with CLONE_CHILD_CLEARTID)
///
/// Writes 0 to the address and returns the PID to wake (if any).
/// Used when a thread exits with clear_child_tid set.
///
/// # Arguments
/// * `addr` - Address to clear and wake
///
/// # Returns
/// Optional PID to wake
pub fn futex_clear_and_wake(addr: u64) -> Option<Pid> {
    if addr == 0 || addr >= 0x0000_8000_0000_0000 {
        return None;
    }

    // Write 0 to the address
    unsafe {
        let ptr = addr as *mut u32;
        core::ptr::write_volatile(ptr, 0);
    }

    // Wake one waiter
    match futex_wake(addr, 1) {
        Ok(pids) => pids.into_iter().next(),
        Err(_) => None,
    }
}
