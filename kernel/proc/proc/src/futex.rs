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
/// — ThreadRogue: Linux futex opcodes. Private flag (bit 7) means the futex
/// is process-private (not shared via shared memory). We treat both the same
/// since we key by virtual address within the process.
pub mod futex_op {
    pub const FUTEX_WAIT: i32 = 0;
    pub const FUTEX_WAKE: i32 = 1;
    pub const FUTEX_FD: i32 = 2;
    pub const FUTEX_REQUEUE: i32 = 3;
    pub const FUTEX_CMP_REQUEUE: i32 = 4;
    pub const FUTEX_WAKE_OP: i32 = 5;
    pub const FUTEX_LOCK_PI: i32 = 6;
    pub const FUTEX_UNLOCK_PI: i32 = 7;
    pub const FUTEX_TRYLOCK_PI: i32 = 8;
    pub const FUTEX_WAIT_BITSET: i32 = 9;
    pub const FUTEX_WAKE_BITSET: i32 = 10;
    pub const FUTEX_PRIVATE_FLAG: i32 = 128;
    pub const FUTEX_WAIT_PRIVATE: i32 = FUTEX_WAIT | FUTEX_PRIVATE_FLAG;
    pub const FUTEX_WAKE_PRIVATE: i32 = FUTEX_WAKE | FUTEX_PRIVATE_FLAG;
    pub const FUTEX_REQUEUE_PRIVATE: i32 = FUTEX_REQUEUE | FUTEX_PRIVATE_FLAG;
    pub const FUTEX_CMP_REQUEUE_PRIVATE: i32 = FUTEX_CMP_REQUEUE | FUTEX_PRIVATE_FLAG;
    pub const FUTEX_WAIT_BITSET_PRIVATE: i32 = FUTEX_WAIT_BITSET | FUTEX_PRIVATE_FLAG;
    pub const FUTEX_WAKE_BITSET_PRIVATE: i32 = FUTEX_WAKE_BITSET | FUTEX_PRIVATE_FLAG;
    /// Match all bits (default bitmask for FUTEX_WAIT_BITSET)
    pub const FUTEX_BITSET_MATCH_ANY: u32 = 0xFFFFFFFF;
}

/// A waiter on a futex
/// — ThreadRogue: includes bitmask for FUTEX_WAIT_BITSET support.
/// When bitset == FUTEX_BITSET_MATCH_ANY (0xFFFFFFFF), it's a regular FUTEX_WAIT.
#[derive(Debug, Clone, Copy)]
struct FutexWaiter {
    /// PID/TID of the waiting process/thread
    pid: Pid,
    /// Bitmask for FUTEX_WAIT_BITSET (0xFFFFFFFF = match any)
    bitset: u32,
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
        waiters.push(FutexWaiter { pid: current_pid, bitset: futex_op::FUTEX_BITSET_MATCH_ANY });
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

/// Prepare for futex wait with bitmask (FUTEX_WAIT_BITSET)
/// — ThreadRogue: like futex_wait_prepare but only wakes if the wake's bitset
/// overlaps with the waiter's bitset. Used for timed pthread_cond_wait.
pub fn futex_wait_bitset_prepare(
    current_pid: Pid,
    addr: u64,
    expected: u32,
    bitset: u32,
) -> Result<FutexWaitResult, FutexError> {
    if addr >= 0x0000_8000_0000_0000 || addr == 0 { return Err(FutexError::InvalidAddress); }
    if addr % 4 != 0 { return Err(FutexError::InvalidAddress); }
    if bitset == 0 { return Err(FutexError::InvalidOp); }

    let current_val = unsafe { core::ptr::read_volatile(addr as *const u32) };
    if current_val != expected { return Ok(FutexWaitResult::ValueMismatch); }

    let mut queues = FUTEX_QUEUES.lock();
    let waiters = queues.entry(addr).or_insert_with(Vec::new);
    waiters.push(FutexWaiter { pid: current_pid, bitset });
    Ok(FutexWaitResult::ShouldBlock)
}

/// Wake with bitmask (FUTEX_WAKE_BITSET)
/// — ThreadRogue: only wakes waiters whose bitset overlaps with the given bitset.
pub fn futex_wake_bitset(addr: u64, count: i32, bitset: u32) -> Result<Vec<Pid>, FutexError> {
    if addr >= 0x0000_8000_0000_0000 || addr == 0 { return Err(FutexError::InvalidAddress); }
    if bitset == 0 { return Err(FutexError::InvalidOp); }

    let mut queues = FUTEX_QUEUES.lock();
    if let Some(waiters) = queues.get_mut(&addr) {
        let mut woken = Vec::new();
        let mut remaining = Vec::new();
        for w in waiters.drain(..) {
            if woken.len() < count as usize && (w.bitset & bitset) != 0 {
                woken.push(w.pid);
            } else {
                remaining.push(w);
            }
        }
        *waiters = remaining;
        if waiters.is_empty() { queues.remove(&addr); }
        Ok(woken)
    } else {
        Ok(Vec::new())
    }
}

/// Requeue waiters from one futex to another (FUTEX_REQUEUE)
/// — ThreadRogue: wakes up to `wake_count` waiters on `addr`, then moves
/// up to `requeue_count` remaining waiters to `addr2`'s queue.
/// Used by pthread_cond_broadcast to avoid thundering herd.
pub fn futex_requeue(
    addr: u64,
    wake_count: i32,
    addr2: u64,
    requeue_count: i32,
) -> Result<Vec<Pid>, FutexError> {
    if addr >= 0x0000_8000_0000_0000 || addr == 0 { return Err(FutexError::InvalidAddress); }
    if addr2 >= 0x0000_8000_0000_0000 || addr2 == 0 { return Err(FutexError::InvalidAddress); }

    let mut queues = FUTEX_QUEUES.lock();
    let mut woken = Vec::new();

    if let Some(waiters) = queues.get_mut(&addr) {
        // Wake up to wake_count
        let to_wake = wake_count.min(waiters.len() as i32) as usize;
        for w in waiters.drain(..to_wake) {
            woken.push(w.pid);
        }

        // Move up to requeue_count to addr2
        let to_requeue = requeue_count.min(waiters.len() as i32) as usize;
        let requeued: Vec<FutexWaiter> = waiters.drain(..to_requeue).collect();

        if waiters.is_empty() { queues.remove(&addr); }

        // Add requeued waiters to addr2's queue
        if !requeued.is_empty() {
            let q2 = queues.entry(addr2).or_insert_with(Vec::new);
            q2.extend(requeued);
        }
    }

    Ok(woken)
}

/// Compare-and-requeue (FUTEX_CMP_REQUEUE)
/// — ThreadRogue: like futex_requeue but first checks that *addr == expected.
/// This prevents races where the condition variable is signaled between the
/// user-space check and the kernel requeue operation.
pub fn futex_cmp_requeue(
    addr: u64,
    wake_count: i32,
    addr2: u64,
    requeue_count: i32,
    expected: u32,
) -> Result<Vec<Pid>, FutexError> {
    if addr >= 0x0000_8000_0000_0000 || addr == 0 { return Err(FutexError::InvalidAddress); }

    // Check the value BEFORE doing anything
    let current_val = unsafe { core::ptr::read_volatile(addr as *const u32) };
    if current_val != expected {
        return Err(FutexError::WouldBlock); // EAGAIN
    }

    futex_requeue(addr, wake_count, addr2, requeue_count)
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
