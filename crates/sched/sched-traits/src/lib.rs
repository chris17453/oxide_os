//! Scheduler trait definitions for EFFLUX OS
//!
//! Provides architecture-independent interfaces for thread scheduling.

#![no_std]

use os_core::VirtAddr;

/// Thread identifier
pub type ThreadId = u64;

/// Thread state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadState {
    /// Thread is currently running on a CPU
    Running,
    /// Thread is ready to run
    Ready,
    /// Thread is blocked waiting for something
    Blocked,
    /// Thread has exited
    Zombie,
}

/// Thread priority (0 = highest, 31 = lowest)
pub type Priority = u8;

/// Default priority for new threads
pub const DEFAULT_PRIORITY: Priority = 16;

/// Thread trait - represents a schedulable entity
pub trait Thread: Sized {
    /// Architecture-specific context type
    type Context: Context;

    /// Get the thread ID
    fn tid(&self) -> ThreadId;

    /// Get the thread state
    fn state(&self) -> ThreadState;

    /// Set the thread state
    fn set_state(&mut self, state: ThreadState);

    /// Get the thread priority
    fn priority(&self) -> Priority;

    /// Get a reference to the thread's context
    fn context(&self) -> &Self::Context;

    /// Get a mutable reference to the thread's context
    fn context_mut(&mut self) -> &mut Self::Context;

    /// Get the kernel stack top address
    fn kernel_stack_top(&self) -> VirtAddr;
}

/// Context trait - represents saved CPU state
pub trait Context: Clone + Default {
    /// Create a new context for a thread
    ///
    /// - `entry`: The function the thread will start executing
    /// - `stack_top`: The top of the thread's kernel stack
    /// - `arg`: Argument to pass to the entry function
    fn new(entry: fn(usize) -> !, stack_top: usize, arg: usize) -> Self;

    /// Get the stack pointer from this context
    fn stack_pointer(&self) -> usize;
}

/// Scheduler trait - manages thread execution
pub trait Scheduler {
    /// Thread type used by this scheduler
    type Thread: Thread;

    /// Add a thread to the scheduler
    fn add(&mut self, thread: Self::Thread);

    /// Remove a thread from the scheduler
    fn remove(&mut self, tid: ThreadId) -> Option<Self::Thread>;

    /// Get the next thread to run
    ///
    /// Returns None if no threads are ready to run.
    fn next(&mut self) -> Option<&mut Self::Thread>;

    /// Get a reference to a thread by ID
    fn get(&self, tid: ThreadId) -> Option<&Self::Thread>;

    /// Get a mutable reference to a thread by ID
    fn get_mut(&mut self, tid: ThreadId) -> Option<&mut Self::Thread>;

    /// Get the currently running thread (if any)
    fn current(&self) -> Option<&Self::Thread>;

    /// Get a mutable reference to the currently running thread
    fn current_mut(&mut self) -> Option<&mut Self::Thread>;

    /// Mark a thread as the currently running thread
    fn set_current(&mut self, tid: ThreadId);

    /// Called when a timer tick occurs
    ///
    /// Returns true if a context switch should occur.
    fn tick(&mut self) -> bool;

    /// Number of threads in the scheduler
    fn len(&self) -> usize;

    /// Check if the scheduler is empty
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Context switch operations (provided by architecture crate)
pub trait ContextSwitch {
    /// Context type
    type Context: Context;

    /// Perform a context switch from `old` to `new`
    ///
    /// # Safety
    /// - Both contexts must be valid
    /// - The old context will be saved and the new context will be restored
    unsafe fn switch(old: *mut Self::Context, new: *const Self::Context);
}
