//! Wait implementation
//!
//! Implements the wait() and waitpid() system calls for reaping child processes.

use crate::process_table;
use proc_traits::{Pid, ProcessState};

/// Wait options
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaitOptions {
    /// Don't block if no child has exited
    pub nohang: bool,
    /// Also report stopped children
    pub untraced: bool,
}

impl WaitOptions {
    pub const NONE: Self = Self {
        nohang: false,
        untraced: false,
    };

    pub const WNOHANG: Self = Self {
        nohang: true,
        untraced: false,
    };
}

impl From<i32> for WaitOptions {
    fn from(flags: i32) -> Self {
        Self {
            nohang: flags & 1 != 0,   // WNOHANG = 1
            untraced: flags & 2 != 0, // WUNTRACED = 2
        }
    }
}

/// Wait error
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitError {
    /// No children
    NoChildren,
    /// Would block (with WNOHANG)
    WouldBlock,
    /// Interrupted
    Interrupted,
    /// Invalid PID
    InvalidPid,
}

/// Wait result
#[derive(Debug, Clone, Copy)]
pub struct WaitResult {
    /// PID of the child that exited
    pub pid: Pid,
    /// Exit status
    pub status: i32,
}

/// Wait for any child process
///
/// Blocks until a child process exits (or returns immediately with WNOHANG).
/// Returns the PID and exit status of the reaped child.
pub fn do_wait(parent_pid: Pid, options: WaitOptions) -> Result<WaitResult, WaitError> {
    do_waitpid(parent_pid, -1, options)
}

/// Wait for a specific child process
///
/// # Arguments
/// * `parent_pid` - The waiting process's PID
/// * `wait_pid` - PID to wait for:
///   - > 0: Wait for specific child
///   - -1: Wait for any child
///   - 0: Wait for any child in same process group
///   - < -1: Wait for any child in process group |pid|
/// * `options` - Wait options
pub fn do_waitpid(
    parent_pid: Pid,
    wait_pid: i32,
    options: WaitOptions,
) -> Result<WaitResult, WaitError> {
    let table = process_table();

    loop {
        // Find matching zombie children
        let zombie_pids: alloc::vec::Vec<Pid> = table
            .find_children(parent_pid)
            .into_iter()
            .filter(|&child_pid| {
                // Check if PID matches wait criteria
                let matches = match wait_pid {
                    -1 => true, // Any child
                    0 => {
                        // Same process group
                        if let Some(parent) = table.get(parent_pid) {
                            if let Some(child) = table.get(child_pid) {
                                parent.lock().pgid() == child.lock().pgid()
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    }
                    n if n > 0 => child_pid == n as Pid, // Specific PID
                    n => {
                        // Process group |n|
                        let target_pgid = (-n) as Pid;
                        if let Some(child) = table.get(child_pid) {
                            child.lock().pgid() == target_pgid
                        } else {
                            false
                        }
                    }
                };

                if !matches {
                    return false;
                }

                // Check if zombie
                if let Some(child) = table.get(child_pid) {
                    child.lock().state() == ProcessState::Zombie
                } else {
                    false
                }
            })
            .collect();

        // If we have a zombie, reap it
        if let Some(&zombie_pid) = zombie_pids.first() {
            // Get exit status before removing
            let status = if let Some(child) = table.get(zombie_pid) {
                child.lock().exit_status()
            } else {
                0
            };

            // Remove from parent's children list
            if let Some(parent) = table.get(parent_pid) {
                parent.lock().remove_child(zombie_pid);
            }

            // Remove from process table
            table.remove(zombie_pid);

            return Ok(WaitResult {
                pid: zombie_pid,
                status,
            });
        }

        // Check if we have any matching children at all
        let has_children = !table.find_children(parent_pid).is_empty();
        if !has_children {
            return Err(WaitError::NoChildren);
        }

        // For specific PID, check if it exists and is our child
        if wait_pid > 0 {
            let target_pid = wait_pid as Pid;
            let is_our_child = table.find_children(parent_pid).contains(&target_pid);

            if !is_our_child {
                return Err(WaitError::InvalidPid);
            }
        }

        // Return WouldBlock - caller (kernel) must handle yielding/blocking
        // We can't spin here because:
        // 1. We're in kernel mode
        // 2. Scheduler only preempts user mode
        // 3. Child processes never get scheduled -> deadlock
        return Err(WaitError::WouldBlock);
    }
}

/// Check if a process has any children
pub fn has_children(pid: Pid) -> bool {
    !process_table().find_children(pid).is_empty()
}

/// Check if a specific child exists
pub fn is_child(parent_pid: Pid, child_pid: Pid) -> bool {
    process_table()
        .find_children(parent_pid)
        .contains(&child_pid)
}
