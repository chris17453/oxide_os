//! Wait types
//!
//! Defines types for the wait() and waitpid() system calls.
//! The actual wait implementation is in kernel/src/process.rs which has
//! access to the scheduler.

use proc_traits::Pid;

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
