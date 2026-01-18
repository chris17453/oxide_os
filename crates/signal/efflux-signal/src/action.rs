//! Signal action (handler) definitions
//!
//! Defines SigAction structure for signal handlers.

use bitflags::bitflags;
use crate::sigset::SigSet;

/// Signal handler type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SigHandler {
    /// Default action (SIG_DFL = 0)
    Default,
    /// Ignore the signal (SIG_IGN = 1)
    Ignore,
    /// User-defined handler function
    Handler(u64),
}

impl SigHandler {
    /// SIG_DFL constant
    pub const SIG_DFL: u64 = 0;
    /// SIG_IGN constant
    pub const SIG_IGN: u64 = 1;

    /// Create from raw handler pointer
    pub fn from_raw(handler: u64) -> Self {
        match handler {
            0 => SigHandler::Default,
            1 => SigHandler::Ignore,
            addr => SigHandler::Handler(addr),
        }
    }

    /// Convert to raw handler pointer
    pub fn to_raw(&self) -> u64 {
        match self {
            SigHandler::Default => 0,
            SigHandler::Ignore => 1,
            SigHandler::Handler(addr) => *addr,
        }
    }

    /// Check if this is a user handler
    pub fn is_user_handler(&self) -> bool {
        matches!(self, SigHandler::Handler(_))
    }
}

bitflags! {
    /// Signal action flags
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SigFlags: u64 {
        /// Don't send SIGCHLD when children stop
        const SA_NOCLDSTOP = 0x00000001;
        /// Don't create zombie on child death
        const SA_NOCLDWAIT = 0x00000002;
        /// Use sa_sigaction handler (3-arg)
        const SA_SIGINFO = 0x00000004;
        /// Use alternate signal stack
        const SA_ONSTACK = 0x08000000;
        /// Restart interrupted syscall
        const SA_RESTART = 0x10000000;
        /// Don't block signal in handler
        const SA_NODEFER = 0x40000000;
        /// Reset to SIG_DFL after handler
        const SA_RESETHAND = 0x80000000;
        /// Restorer function provided
        const SA_RESTORER = 0x04000000;
    }
}

/// Signal action structure (matches Linux sigaction)
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct SigAction {
    /// Signal handler
    pub sa_handler: u64,
    /// Signal flags
    pub sa_flags: u64,
    /// Signal restorer (used by libc)
    pub sa_restorer: u64,
    /// Signals to block during handler
    pub sa_mask: SigSet,
}

impl Default for SigAction {
    fn default() -> Self {
        SigAction {
            sa_handler: SigHandler::SIG_DFL,
            sa_flags: 0,
            sa_restorer: 0,
            sa_mask: SigSet::empty(),
        }
    }
}

impl SigAction {
    /// Create a new signal action with default handler
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the handler type
    pub fn handler(&self) -> SigHandler {
        SigHandler::from_raw(self.sa_handler)
    }

    /// Set the handler
    pub fn set_handler(&mut self, handler: SigHandler) {
        self.sa_handler = handler.to_raw();
    }

    /// Get flags
    pub fn flags(&self) -> SigFlags {
        SigFlags::from_bits_truncate(self.sa_flags)
    }

    /// Check if SA_SIGINFO flag is set
    pub fn is_siginfo(&self) -> bool {
        self.flags().contains(SigFlags::SA_SIGINFO)
    }

    /// Check if SA_RESTART flag is set
    pub fn is_restart(&self) -> bool {
        self.flags().contains(SigFlags::SA_RESTART)
    }

    /// Check if SA_NODEFER flag is set
    pub fn is_nodefer(&self) -> bool {
        self.flags().contains(SigFlags::SA_NODEFER)
    }

    /// Check if SA_RESETHAND flag is set
    pub fn is_resethand(&self) -> bool {
        self.flags().contains(SigFlags::SA_RESETHAND)
    }

    /// Check if SA_ONSTACK flag is set
    pub fn is_onstack(&self) -> bool {
        self.flags().contains(SigFlags::SA_ONSTACK)
    }
}

/// Signal info structure (for SA_SIGINFO handlers)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(C)]
pub struct SigInfo {
    /// Signal number
    pub si_signo: i32,
    /// Error number (errno)
    pub si_errno: i32,
    /// Signal code
    pub si_code: i32,
    /// Padding for alignment
    _pad0: i32,
    /// Sending process ID
    pub si_pid: u32,
    /// Sending user ID
    pub si_uid: u32,
    /// Exit status or signal
    pub si_status: i32,
    /// Padding
    _pad1: i32,
    /// User time consumed
    pub si_utime: u64,
    /// System time consumed
    pub si_stime: u64,
    /// Signal value (for real-time signals)
    pub si_value: u64,
    /// Fault address (for SIGSEGV, SIGBUS)
    pub si_addr: u64,
    /// Reserved for future use
    _reserved: [u64; 4],
}

impl SigInfo {
    /// Create a new siginfo for a given signal
    pub fn new(signo: i32) -> Self {
        SigInfo {
            si_signo: signo,
            ..Default::default()
        }
    }

    /// Create siginfo for a kill() syscall
    pub fn kill(signo: i32, pid: u32, uid: u32) -> Self {
        SigInfo {
            si_signo: signo,
            si_code: SI_USER,
            si_pid: pid,
            si_uid: uid,
            ..Default::default()
        }
    }

    /// Create siginfo for a child signal
    pub fn child(signo: i32, pid: u32, uid: u32, status: i32) -> Self {
        SigInfo {
            si_signo: signo,
            si_code: CLD_EXITED,
            si_pid: pid,
            si_uid: uid,
            si_status: status,
            ..Default::default()
        }
    }

    /// Create siginfo for a fault
    pub fn fault(signo: i32, addr: u64, code: i32) -> Self {
        SigInfo {
            si_signo: signo,
            si_code: code,
            si_addr: addr,
            ..Default::default()
        }
    }
}

// Signal codes (si_code)
pub const SI_USER: i32 = 0;      // Sent by kill()
pub const SI_KERNEL: i32 = 128;  // Sent by kernel
pub const SI_QUEUE: i32 = -1;    // Sent by sigqueue()
pub const SI_TIMER: i32 = -2;    // Timer expired
pub const SI_MESGQ: i32 = -3;    // Message queue state changed
pub const SI_ASYNCIO: i32 = -4;  // AIO completed
pub const SI_SIGIO: i32 = -5;    // SIGIO queued
pub const SI_TKILL: i32 = -6;    // Sent by tkill()

// SIGCHLD codes
pub const CLD_EXITED: i32 = 1;    // Child exited
pub const CLD_KILLED: i32 = 2;    // Child killed by signal
pub const CLD_DUMPED: i32 = 3;    // Child dumped core
pub const CLD_TRAPPED: i32 = 4;   // Traced child trapped
pub const CLD_STOPPED: i32 = 5;   // Child stopped
pub const CLD_CONTINUED: i32 = 6; // Child continued

// SIGSEGV codes
pub const SEGV_MAPERR: i32 = 1;   // Address not mapped
pub const SEGV_ACCERR: i32 = 2;   // Invalid permissions

// SIGBUS codes
pub const BUS_ADRALN: i32 = 1;    // Invalid address alignment
pub const BUS_ADRERR: i32 = 2;    // Non-existent physical address
pub const BUS_OBJERR: i32 = 3;    // Object-specific hardware error
