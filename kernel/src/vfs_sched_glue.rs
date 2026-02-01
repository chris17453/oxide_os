//! VFS-Scheduler Glue Functions
//!
//! Provides extern "Rust" functions that break the circular dependency between
//! vfs and sched crates. The VFS declares these as extern, and we implement them
//! here in the kernel which has access to both crates.

use sched::{self, TaskState};

/// Block the current task in TASK_INTERRUPTIBLE state
///
/// Called by VFS when a pipe read/write would block.
#[unsafe(no_mangle)]
pub extern "Rust" fn sched_block_interruptible() {
    sched::block_current(TaskState::TASK_INTERRUPTIBLE);
}

/// Wake up a task by PID
///
/// Called by VFS when data becomes available or space frees up.
#[unsafe(no_mangle)]
pub extern "Rust" fn sched_wake_up(pid: u32) {
    sched::wake_up(pid);
}

/// Get the current task's PID
///
/// Called by VFS to add the current task to a wait queue.
#[unsafe(no_mangle)]
pub extern "Rust" fn sched_current_pid() -> Option<u32> {
    sched::current_pid()
}
