//! Signal handling
//!
//! Signal numbers and handling functions.

/// Signal numbers
pub const SIGHUP: i32 = 1;
pub const SIGINT: i32 = 2;
pub const SIGQUIT: i32 = 3;
pub const SIGILL: i32 = 4;
pub const SIGTRAP: i32 = 5;
pub const SIGABRT: i32 = 6;
pub const SIGBUS: i32 = 7;
pub const SIGFPE: i32 = 8;
pub const SIGKILL: i32 = 9;
pub const SIGUSR1: i32 = 10;
pub const SIGSEGV: i32 = 11;
pub const SIGUSR2: i32 = 12;
pub const SIGPIPE: i32 = 13;
pub const SIGALRM: i32 = 14;
pub const SIGTERM: i32 = 15;
pub const SIGSTKFLT: i32 = 16;
pub const SIGCHLD: i32 = 17;
pub const SIGCONT: i32 = 18;
pub const SIGSTOP: i32 = 19;
pub const SIGTSTP: i32 = 20;
pub const SIGTTIN: i32 = 21;
pub const SIGTTOU: i32 = 22;
pub const SIGURG: i32 = 23;
pub const SIGXCPU: i32 = 24;
pub const SIGXFSZ: i32 = 25;
pub const SIGVTALRM: i32 = 26;
pub const SIGPROF: i32 = 27;
pub const SIGWINCH: i32 = 28;
pub const SIGIO: i32 = 29;
pub const SIGPWR: i32 = 30;
pub const SIGSYS: i32 = 31;

/// Special signal handlers
pub const SIG_DFL: u64 = 0;
pub const SIG_IGN: u64 = 1;

/// Signal action flags
pub const SA_NOCLDSTOP: u64 = 0x00000001;
pub const SA_NOCLDWAIT: u64 = 0x00000002;
pub const SA_SIGINFO: u64 = 0x00000004;
pub const SA_ONSTACK: u64 = 0x08000000;
pub const SA_RESTART: u64 = 0x10000000;
pub const SA_NODEFER: u64 = 0x40000000;
pub const SA_RESETHAND: u64 = 0x80000000;

/// Signal set
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct SigSet {
    pub bits: u64,
}

impl SigSet {
    pub const fn empty() -> Self {
        SigSet { bits: 0 }
    }

    pub fn add(&mut self, sig: i32) {
        if sig >= 1 && sig <= 64 {
            self.bits |= 1 << (sig - 1);
        }
    }

    pub fn remove(&mut self, sig: i32) {
        if sig >= 1 && sig <= 64 {
            self.bits &= !(1 << (sig - 1));
        }
    }

    pub fn contains(&self, sig: i32) -> bool {
        if sig >= 1 && sig <= 64 {
            (self.bits & (1 << (sig - 1))) != 0
        } else {
            false
        }
    }
}

/// Signal action
#[derive(Clone, Copy)]
#[repr(C)]
pub struct SigAction {
    pub sa_handler: u64,
    pub sa_flags: u64,
    pub sa_restorer: u64,
    pub sa_mask: SigSet,
}

impl Default for SigAction {
    fn default() -> Self {
        SigAction {
            sa_handler: SIG_DFL,
            sa_flags: 0,
            sa_restorer: 0,
            sa_mask: SigSet::empty(),
        }
    }
}

/// Send signal to process
pub fn kill(pid: i32, sig: i32) -> i32 {
    crate::syscall::sys_kill(pid, sig)
}

/// Send signal to self
pub fn raise(sig: i32) -> i32 {
    let pid = crate::syscall::sys_getpid();
    kill(pid, sig)
}

/// Set signal handler (simple interface)
/// Returns previous handler on success, SIG_ERR on error
pub fn signal(sig: i32, handler: u64) -> u64 {
    let new_action = SigAction {
        sa_handler: handler,
        sa_flags: SA_RESTART,
        sa_restorer: 0,
        sa_mask: SigSet::empty(),
    };

    let mut old_action = SigAction::default();

    if sigaction(sig, Some(&new_action), Some(&mut old_action)) < 0 {
        return !0u64; // SIG_ERR
    }

    old_action.sa_handler
}

/// Set signal action
pub fn sigaction(sig: i32, act: Option<&SigAction>, oldact: Option<&mut SigAction>) -> i32 {
    crate::syscall::syscall3(
        crate::syscall::nr::SIGACTION,
        sig as usize,
        act.map_or(0, |a| a as *const _ as usize),
        oldact.map_or(0, |a| a as *mut _ as usize),
    ) as i32
}
