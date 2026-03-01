//! Signal handling for OXIDE OS
//!
//! This crate provides POSIX-compatible signal handling including:
//! - Signal numbers and default actions
//! - Signal sets (masks) for blocking
//! - Signal actions (handlers)
//! - Pending signal queues

#![no_std]

extern crate alloc;

pub mod action;
pub mod delivery;
pub mod pending;
pub mod signal;
pub mod sigset;

// Re-export commonly used types
pub use action::{
    BUS_ADRALN, BUS_ADRERR, BUS_OBJERR, CLD_CONTINUED, CLD_DUMPED, CLD_EXITED, CLD_KILLED,
    CLD_STOPPED, CLD_TRAPPED, SEGV_ACCERR, SEGV_MAPERR, SI_ASYNCIO, SI_KERNEL, SI_MESGQ, SI_QUEUE,
    SI_SIGIO, SI_TIMER, SI_TKILL, SI_USER,
};
pub use action::{SigAction, SigFlags, SigHandler, SigInfo};
pub use delivery::{
    SavedRegisters, SignalFrame, SignalResult, determine_action, restore_from_frame,
    set_sigreturn_frame, setup_signal_handler, should_interrupt_for_signal, take_sigreturn_frame,
};
pub use pending::{PendingSignal, PendingSignals};
pub use signal::{
    DefaultAction, NSIG, SIGABRT, SIGALRM, SIGBUS, SIGCHLD, SIGCONT, SIGFPE, SIGHUP, SIGILL,
    SIGINT, SIGIO, SIGKILL, SIGPIPE, SIGPROF, SIGPWR, SIGQUIT, SIGRTMAX, SIGRTMIN, SIGSEGV,
    SIGSTKFLT, SIGSTOP, SIGSYS, SIGTERM, SIGTRAP, SIGTSTP, SIGTTIN, SIGTTOU, SIGURG, SIGUSR1,
    SIGUSR2, SIGVTALRM, SIGWINCH, SIGXCPU, SIGXFSZ, can_block, can_catch, default_action, is_valid,
    signal_name,
};
pub use sigset::{SigHow, SigSet, SigSetIter};
