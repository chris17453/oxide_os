//! Wait types
//!
//! Defines types for the wait() and waitpid() system calls.
//! The actual wait implementation is in kernel/src/process.rs which has
//! access to the scheduler.
//!
//! — ThreadRogue: reaping the fallen, thawing the frozen

use proc_traits::Pid;

/// Wait options
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaitOptions {
    /// Don't block if no child has exited
    pub nohang: bool,
    /// Also report stopped children (WUNTRACED)
    pub untraced: bool,
    /// Also report continued children (WCONTINUED)
    pub continued: bool,
}

impl WaitOptions {
    pub const NONE: Self = Self {
        nohang: false,
        untraced: false,
        continued: false,
    };

    pub const WNOHANG: Self = Self {
        nohang: true,
        untraced: false,
        continued: false,
    };
}

impl From<i32> for WaitOptions {
    fn from(flags: i32) -> Self {
        Self {
            nohang: flags & 1 != 0,   // WNOHANG = 1
            untraced: flags & 2 != 0, // WUNTRACED = 2
            continued: flags & 8 != 0, // WCONTINUED = 8
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
    /// PID of the child that changed state
    pub pid: Pid,
    /// Encoded status word (Linux waitpid format)
    pub status: i32,
}

impl WaitResult {
    /// Build status for normal exit: bits [15:8] = exit code, bits [7:0] = 0
    pub fn exited(pid: Pid, exit_code: i32) -> Self {
        Self {
            pid,
            status: (exit_code & 0xFF) << 8,
        }
    }

    /// Build status for signal termination: bits [6:0] = signal number
    pub fn signaled(pid: Pid, signo: i32) -> Self {
        Self {
            pid,
            status: signo & 0x7F,
        }
    }

    /// Build status for stopped child: 0x7F in low byte, signal in bits [15:8]
    pub fn stopped(pid: Pid, stop_sig: i32) -> Self {
        Self {
            pid,
            status: ((stop_sig & 0xFF) << 8) | 0x7F,
        }
    }

    /// Build status for continued child: 0xFFFF
    pub fn continued(pid: Pid) -> Self {
        Self {
            pid,
            status: 0xFFFF,
        }
    }
}
