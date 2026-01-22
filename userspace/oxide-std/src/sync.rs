//! Synchronization primitives for OXIDE OS
//!
//! Provides std::sync-like APIs using futex syscalls.

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicU32, Ordering};

// ============================================================================
// Mutex
// ============================================================================

/// A mutual exclusion primitive useful for protecting shared data
pub struct Mutex<T: ?Sized> {
    /// Lock state: 0 = unlocked, 1 = locked, 2 = locked with waiters
    state: AtomicU32,
    /// The protected data
    data: UnsafeCell<T>,
}

// Safety: Mutex provides exclusive access to T
unsafe impl<T: ?Sized + Send> Send for Mutex<T> {}
unsafe impl<T: ?Sized + Send> Sync for Mutex<T> {}

impl<T> Mutex<T> {
    /// Creates a new mutex in an unlocked state
    pub const fn new(data: T) -> Self {
        Mutex {
            state: AtomicU32::new(0),
            data: UnsafeCell::new(data),
        }
    }

    /// Consumes this mutex, returning the underlying data
    pub fn into_inner(self) -> T {
        self.data.into_inner()
    }
}

impl<T: ?Sized> Mutex<T> {
    /// Acquires a mutex, blocking the current thread until it is able to do so
    pub fn lock(&self) -> MutexGuard<'_, T> {
        // Fast path: try to acquire immediately
        if self
            .state
            .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            return MutexGuard { mutex: self };
        }

        // Slow path: contended lock
        self.lock_contended();
        MutexGuard { mutex: self }
    }

    #[cold]
    fn lock_contended(&self) {
        loop {
            // Try to acquire, setting state to 2 (locked with waiters)
            let mut state = self.state.load(Ordering::Relaxed);

            // If unlocked, try to acquire
            if state == 0 {
                if self
                    .state
                    .compare_exchange(0, 2, Ordering::Acquire, Ordering::Relaxed)
                    .is_ok()
                {
                    return;
                }
                continue;
            }

            // Mark that there are waiters
            if state == 1 {
                self.state
                    .compare_exchange(1, 2, Ordering::Relaxed, Ordering::Relaxed)
                    .ok();
                state = 2;
            }

            // Wait using futex
            if state == 2 {
                unsafe {
                    let ptr = &self.state as *const AtomicU32 as *mut u32;
                    libc::sys_futex_wait(ptr, 2, 0);
                }
            }

            // Try to acquire again
            if self
                .state
                .compare_exchange(0, 2, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                return;
            }
        }
    }

    /// Attempts to acquire this lock without blocking
    pub fn try_lock(&self) -> Option<MutexGuard<'_, T>> {
        if self
            .state
            .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            Some(MutexGuard { mutex: self })
        } else {
            None
        }
    }

    /// Gets a mutable reference to the underlying data
    pub fn get_mut(&mut self) -> &mut T {
        self.data.get_mut()
    }
}

/// RAII guard for Mutex
pub struct MutexGuard<'a, T: ?Sized + 'a> {
    mutex: &'a Mutex<T>,
}

impl<T: ?Sized> Deref for MutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.mutex.data.get() }
    }
}

impl<T: ?Sized> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.mutex.data.get() }
    }
}

impl<T: ?Sized> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        // Release the lock
        let state = self.mutex.state.swap(0, Ordering::Release);

        // If there were waiters, wake one
        if state == 2 {
            unsafe {
                let ptr = &self.mutex.state as *const AtomicU32 as *mut u32;
                libc::sys_futex_wake(ptr, 1);
            }
        }
    }
}

// ============================================================================
// RwLock
// ============================================================================

/// A reader-writer lock
///
/// Multiple readers can hold the lock simultaneously, but only one writer
/// can hold it at a time. Writers have priority over readers.
pub struct RwLock<T: ?Sized> {
    /// Lock state: 0 = unlocked, positive = readers, -1 = writer
    /// Using u32 for futex compatibility, interpret high bit as sign
    state: AtomicU32,
    /// The protected data
    data: UnsafeCell<T>,
}

const WRITER_BIT: u32 = 0x8000_0000;
const READER_MASK: u32 = 0x7FFF_FFFF;

unsafe impl<T: ?Sized + Send> Send for RwLock<T> {}
unsafe impl<T: ?Sized + Send + Sync> Sync for RwLock<T> {}

impl<T> RwLock<T> {
    /// Creates a new RwLock
    pub const fn new(data: T) -> Self {
        RwLock {
            state: AtomicU32::new(0),
            data: UnsafeCell::new(data),
        }
    }

    /// Consumes this RwLock, returning the underlying data
    pub fn into_inner(self) -> T {
        self.data.into_inner()
    }
}

impl<T: ?Sized> RwLock<T> {
    /// Locks this RwLock for shared read access
    pub fn read(&self) -> RwLockReadGuard<'_, T> {
        loop {
            let state = self.state.load(Ordering::Relaxed);

            // If no writer, try to add a reader
            if state & WRITER_BIT == 0 {
                let new_state = state + 1;
                if self
                    .state
                    .compare_exchange(state, new_state, Ordering::Acquire, Ordering::Relaxed)
                    .is_ok()
                {
                    return RwLockReadGuard { lock: self };
                }
            } else {
                // Writer holds lock, wait
                unsafe {
                    let ptr = &self.state as *const AtomicU32 as *mut u32;
                    libc::sys_futex_wait(ptr, state, 0);
                }
            }
        }
    }

    /// Locks this RwLock for exclusive write access
    pub fn write(&self) -> RwLockWriteGuard<'_, T> {
        loop {
            let state = self.state.load(Ordering::Relaxed);

            // If unlocked, try to acquire writer
            if state == 0 {
                if self
                    .state
                    .compare_exchange(0, WRITER_BIT, Ordering::Acquire, Ordering::Relaxed)
                    .is_ok()
                {
                    return RwLockWriteGuard { lock: self };
                }
            } else {
                // Readers or writer hold lock, wait
                unsafe {
                    let ptr = &self.state as *const AtomicU32 as *mut u32;
                    libc::sys_futex_wait(ptr, state, 0);
                }
            }
        }
    }

    /// Attempts to acquire read access without blocking
    pub fn try_read(&self) -> Option<RwLockReadGuard<'_, T>> {
        let state = self.state.load(Ordering::Relaxed);
        if state & WRITER_BIT == 0 {
            let new_state = state + 1;
            if self
                .state
                .compare_exchange(state, new_state, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                return Some(RwLockReadGuard { lock: self });
            }
        }
        None
    }

    /// Attempts to acquire write access without blocking
    pub fn try_write(&self) -> Option<RwLockWriteGuard<'_, T>> {
        if self
            .state
            .compare_exchange(0, WRITER_BIT, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            Some(RwLockWriteGuard { lock: self })
        } else {
            None
        }
    }

    /// Gets a mutable reference to the underlying data
    pub fn get_mut(&mut self) -> &mut T {
        self.data.get_mut()
    }
}

/// RAII guard for read access
pub struct RwLockReadGuard<'a, T: ?Sized + 'a> {
    lock: &'a RwLock<T>,
}

impl<T: ?Sized> Deref for RwLockReadGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T: ?Sized> Drop for RwLockReadGuard<'_, T> {
    fn drop(&mut self) {
        let prev = self.lock.state.fetch_sub(1, Ordering::Release);

        // If we were the last reader, wake a waiting writer
        if prev == 1 {
            unsafe {
                let ptr = &self.lock.state as *const AtomicU32 as *mut u32;
                libc::sys_futex_wake(ptr, 1);
            }
        }
    }
}

/// RAII guard for write access
pub struct RwLockWriteGuard<'a, T: ?Sized + 'a> {
    lock: &'a RwLock<T>,
}

impl<T: ?Sized> Deref for RwLockWriteGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T: ?Sized> DerefMut for RwLockWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T: ?Sized> Drop for RwLockWriteGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.state.store(0, Ordering::Release);

        // Wake all waiters (could be readers or a writer)
        unsafe {
            let ptr = &self.lock.state as *const AtomicU32 as *mut u32;
            libc::sys_futex_wake(ptr, i32::MAX as u32);
        }
    }
}

// ============================================================================
// Condvar
// ============================================================================

/// A Condition Variable
///
/// Used to block threads and wait for an event to occur.
pub struct Condvar {
    /// Sequence number for spurious wakeup prevention
    seq: AtomicU32,
}

impl Condvar {
    /// Creates a new condition variable
    pub const fn new() -> Self {
        Condvar {
            seq: AtomicU32::new(0),
        }
    }

    /// Blocks the current thread until this condition variable receives a notification
    pub fn wait<'a, T>(&self, guard: MutexGuard<'a, T>) -> MutexGuard<'a, T> {
        let seq = self.seq.load(Ordering::Relaxed);

        // Release the mutex
        let mutex = guard.mutex;
        drop(guard);

        // Wait for notification
        unsafe {
            let ptr = &self.seq as *const AtomicU32 as *mut u32;
            libc::sys_futex_wait(ptr, seq, 0);
        }

        // Re-acquire the mutex
        mutex.lock()
    }

    /// Wakes up one blocked thread on this condvar
    pub fn notify_one(&self) {
        self.seq.fetch_add(1, Ordering::Release);
        unsafe {
            let ptr = &self.seq as *const AtomicU32 as *mut u32;
            libc::sys_futex_wake(ptr, 1);
        }
    }

    /// Wakes up all blocked threads on this condvar
    pub fn notify_all(&self) {
        self.seq.fetch_add(1, Ordering::Release);
        unsafe {
            let ptr = &self.seq as *const AtomicU32 as *mut u32;
            libc::sys_futex_wake(ptr, i32::MAX as u32);
        }
    }
}

impl Default for Condvar {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Once
// ============================================================================

/// A synchronization primitive for running one-time global initialization
pub struct Once {
    state: AtomicU32,
}

const ONCE_INCOMPLETE: u32 = 0;
const ONCE_RUNNING: u32 = 1;
const ONCE_COMPLETE: u32 = 2;

impl Once {
    /// Creates a new Once instance
    pub const fn new() -> Self {
        Once {
            state: AtomicU32::new(ONCE_INCOMPLETE),
        }
    }

    /// Performs an initialization routine once and only once
    pub fn call_once<F: FnOnce()>(&self, f: F) {
        // Fast path: already complete
        if self.state.load(Ordering::Acquire) == ONCE_COMPLETE {
            return;
        }

        self.call_once_slow(f);
    }

    #[cold]
    fn call_once_slow<F: FnOnce()>(&self, f: F) {
        loop {
            let state = self.state.load(Ordering::Relaxed);

            match state {
                ONCE_INCOMPLETE => {
                    // Try to start initialization
                    if self
                        .state
                        .compare_exchange(
                            ONCE_INCOMPLETE,
                            ONCE_RUNNING,
                            Ordering::Acquire,
                            Ordering::Relaxed,
                        )
                        .is_ok()
                    {
                        // We won, run the initialization
                        f();
                        self.state.store(ONCE_COMPLETE, Ordering::Release);

                        // Wake any waiters
                        unsafe {
                            let ptr = &self.state as *const AtomicU32 as *mut u32;
                            libc::sys_futex_wake(ptr, i32::MAX as u32);
                        }
                        return;
                    }
                }
                ONCE_RUNNING => {
                    // Someone else is running, wait
                    unsafe {
                        let ptr = &self.state as *const AtomicU32 as *mut u32;
                        libc::sys_futex_wait(ptr, ONCE_RUNNING, 0);
                    }
                }
                ONCE_COMPLETE => {
                    // Done
                    return;
                }
                _ => unreachable!(),
            }
        }
    }

    /// Returns true if some call_once has completed
    pub fn is_completed(&self) -> bool {
        self.state.load(Ordering::Acquire) == ONCE_COMPLETE
    }
}

impl Default for Once {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Barrier
// ============================================================================

/// A barrier enables multiple threads to synchronize at a single point
pub struct Barrier {
    /// Number of threads needed to trip the barrier
    num_threads: u32,
    /// Count of threads waiting
    count: AtomicU32,
    /// Generation number to handle spurious wakeups
    generation: AtomicU32,
}

impl Barrier {
    /// Creates a new barrier
    pub const fn new(n: usize) -> Self {
        Barrier {
            num_threads: n as u32,
            count: AtomicU32::new(0),
            generation: AtomicU32::new(0),
        }
    }

    /// Blocks the current thread until all threads have reached this point
    pub fn wait(&self) -> BarrierWaitResult {
        let current_gen = self.generation.load(Ordering::Relaxed);
        let prev = self.count.fetch_add(1, Ordering::AcqRel);

        if prev + 1 == self.num_threads {
            // We're the last thread, release everyone
            self.count.store(0, Ordering::Release);
            self.generation.fetch_add(1, Ordering::Release);

            // Wake all waiters
            unsafe {
                let ptr = &self.generation as *const AtomicU32 as *mut u32;
                libc::sys_futex_wake(ptr, i32::MAX as u32);
            }

            BarrierWaitResult { is_leader: true }
        } else {
            // Wait for the generation to change
            loop {
                unsafe {
                    let ptr = &self.generation as *const AtomicU32 as *mut u32;
                    libc::sys_futex_wait(ptr, current_gen, 0);
                }

                if self.generation.load(Ordering::Acquire) != current_gen {
                    break;
                }
            }

            BarrierWaitResult { is_leader: false }
        }
    }
}

/// Result from a barrier wait
pub struct BarrierWaitResult {
    is_leader: bool,
}

impl BarrierWaitResult {
    /// Returns whether this thread is the "leader" for this barrier instance
    pub fn is_leader(&self) -> bool {
        self.is_leader
    }
}

// Re-export Arc from alloc
pub use alloc::sync::Arc;
