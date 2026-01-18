//! Signal numbers and definitions
//!
//! POSIX signal numbers and their default actions.

/// Maximum signal number
pub const NSIG: usize = 64;

/// Real-time signals start here
pub const SIGRTMIN: i32 = 32;
pub const SIGRTMAX: i32 = 64;

// Standard signals (Linux numbering for x86_64)
pub const SIGHUP: i32 = 1;      // Hangup
pub const SIGINT: i32 = 2;      // Interrupt (^C)
pub const SIGQUIT: i32 = 3;     // Quit (^\)
pub const SIGILL: i32 = 4;      // Illegal instruction
pub const SIGTRAP: i32 = 5;     // Trace trap
pub const SIGABRT: i32 = 6;     // Abort
pub const SIGBUS: i32 = 7;      // Bus error
pub const SIGFPE: i32 = 8;      // Floating point exception
pub const SIGKILL: i32 = 9;     // Kill (unblockable)
pub const SIGUSR1: i32 = 10;    // User defined 1
pub const SIGSEGV: i32 = 11;    // Segmentation fault
pub const SIGUSR2: i32 = 12;    // User defined 2
pub const SIGPIPE: i32 = 13;    // Broken pipe
pub const SIGALRM: i32 = 14;    // Alarm clock
pub const SIGTERM: i32 = 15;    // Termination
pub const SIGSTKFLT: i32 = 16;  // Stack fault (unused)
pub const SIGCHLD: i32 = 17;    // Child status change
pub const SIGCONT: i32 = 18;    // Continue if stopped
pub const SIGSTOP: i32 = 19;    // Stop (unblockable)
pub const SIGTSTP: i32 = 20;    // Terminal stop (^Z)
pub const SIGTTIN: i32 = 21;    // Background read
pub const SIGTTOU: i32 = 22;    // Background write
pub const SIGURG: i32 = 23;     // Urgent I/O condition
pub const SIGXCPU: i32 = 24;    // CPU time limit exceeded
pub const SIGXFSZ: i32 = 25;    // File size limit exceeded
pub const SIGVTALRM: i32 = 26;  // Virtual timer alarm
pub const SIGPROF: i32 = 27;    // Profiling timer alarm
pub const SIGWINCH: i32 = 28;   // Window size change
pub const SIGIO: i32 = 29;      // I/O possible
pub const SIGPWR: i32 = 30;     // Power failure
pub const SIGSYS: i32 = 31;     // Bad system call

/// Default action for a signal
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultAction {
    /// Terminate the process
    Terminate,
    /// Terminate and generate core dump
    CoreDump,
    /// Ignore the signal
    Ignore,
    /// Stop the process
    Stop,
    /// Continue if stopped
    Continue,
}

/// Get the default action for a signal
pub fn default_action(sig: i32) -> DefaultAction {
    match sig {
        SIGHUP | SIGINT | SIGPIPE | SIGALRM | SIGTERM |
        SIGUSR1 | SIGUSR2 | SIGPROF | SIGVTALRM | SIGSTKFLT |
        SIGIO | SIGPWR => DefaultAction::Terminate,

        SIGQUIT | SIGILL | SIGTRAP | SIGABRT | SIGBUS |
        SIGFPE | SIGSEGV | SIGXCPU | SIGXFSZ | SIGSYS => DefaultAction::CoreDump,

        SIGCHLD | SIGURG | SIGWINCH => DefaultAction::Ignore,

        SIGSTOP | SIGTSTP | SIGTTIN | SIGTTOU => DefaultAction::Stop,

        SIGCONT => DefaultAction::Continue,

        SIGKILL => DefaultAction::Terminate, // Special: unblockable

        // Real-time signals default to terminate
        _ if sig >= SIGRTMIN && sig <= SIGRTMAX => DefaultAction::Terminate,

        // Unknown signals: ignore
        _ => DefaultAction::Ignore,
    }
}

/// Check if a signal can be caught or blocked
pub fn can_catch(sig: i32) -> bool {
    sig != SIGKILL && sig != SIGSTOP
}

/// Check if a signal can be blocked
pub fn can_block(sig: i32) -> bool {
    sig != SIGKILL && sig != SIGSTOP
}

/// Check if this is a valid signal number
pub fn is_valid(sig: i32) -> bool {
    sig >= 1 && sig <= SIGRTMAX
}

/// Signal name for debugging
pub fn signal_name(sig: i32) -> &'static str {
    match sig {
        SIGHUP => "SIGHUP",
        SIGINT => "SIGINT",
        SIGQUIT => "SIGQUIT",
        SIGILL => "SIGILL",
        SIGTRAP => "SIGTRAP",
        SIGABRT => "SIGABRT",
        SIGBUS => "SIGBUS",
        SIGFPE => "SIGFPE",
        SIGKILL => "SIGKILL",
        SIGUSR1 => "SIGUSR1",
        SIGSEGV => "SIGSEGV",
        SIGUSR2 => "SIGUSR2",
        SIGPIPE => "SIGPIPE",
        SIGALRM => "SIGALRM",
        SIGTERM => "SIGTERM",
        SIGSTKFLT => "SIGSTKFLT",
        SIGCHLD => "SIGCHLD",
        SIGCONT => "SIGCONT",
        SIGSTOP => "SIGSTOP",
        SIGTSTP => "SIGTSTP",
        SIGTTIN => "SIGTTIN",
        SIGTTOU => "SIGTTOU",
        SIGURG => "SIGURG",
        SIGXCPU => "SIGXCPU",
        SIGXFSZ => "SIGXFSZ",
        SIGVTALRM => "SIGVTALRM",
        SIGPROF => "SIGPROF",
        SIGWINCH => "SIGWINCH",
        SIGIO => "SIGIO",
        SIGPWR => "SIGPWR",
        SIGSYS => "SIGSYS",
        _ if sig >= SIGRTMIN && sig <= SIGRTMAX => "SIGRT",
        _ => "UNKNOWN",
    }
}
