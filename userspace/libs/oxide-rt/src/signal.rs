//! Signal handling — sigaction, sigprocmask, and the sigreturn trampoline.
//!
//! — GhostPatch: Signals are UNIX's original async notification mechanism.
//! The sigreturn trampoline is the most cursed piece of userspace code
//! you'll ever write. It restores the pre-signal register state via syscall.

use core::arch::asm;
use crate::syscall::*;
use crate::nr;
use crate::types::SigAction;

// Signal numbers (POSIX standard)
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

pub const SIG_DFL: usize = 0;
pub const SIG_IGN: usize = 1;

// SA_* flags
pub const SA_RESTORER: u64 = 0x04000000;
pub const SA_NOCLDSTOP: u64 = 0x00000001;
pub const SA_NOCLDWAIT: u64 = 0x00000002;
pub const SA_SIGINFO: u64 = 0x00000004;
pub const SA_RESTART: u64 = 0x10000000;
pub const SA_NODEFER: u64 = 0x40000000;
pub const SA_RESETHAND: u64 = 0x80000000;

// SIG_BLOCK, SIG_UNBLOCK, SIG_SETMASK for sigprocmask
pub const SIG_BLOCK: i32 = 0;
pub const SIG_UNBLOCK: i32 = 1;
pub const SIG_SETMASK: i32 = 2;

/// sigaction — examine and change a signal action
pub fn sigaction(signum: i32, act: *const SigAction, oldact: *mut SigAction) -> i32 {
    syscall3(
        nr::SIGACTION,
        signum as usize,
        act as usize,
        oldact as usize,
    ) as i32
}

/// sigprocmask — examine and change blocked signals
pub fn sigprocmask(how: i32, set: *const u64, oldset: *mut u64) -> i32 {
    syscall3(
        nr::SIGPROCMASK,
        how as usize,
        set as usize,
        oldset as usize,
    ) as i32
}

/// kill — send signal to a process
pub fn kill(pid: i32, sig: i32) -> i32 {
    syscall2(nr::KILL, pid as usize, sig as usize) as i32
}

/// The sigreturn trampoline — called by kernel after signal handler returns.
/// — GhostPatch: This function's address is stored in sa_restorer.
/// When the signal handler returns, it falls through to this code which
/// calls SYS_SIGRETURN to restore the pre-signal context. Without this,
/// the process would resume execution at garbage addresses.
#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn __oxide_sigreturn() {
    unsafe {
        asm!(
            "mov rax, {sigreturn_nr}",
            "syscall",
            "ud2",
            sigreturn_nr = const nr::SIGRETURN,
            options(noreturn),
        );
    }
}

/// Install a signal handler with proper restorer setup
pub fn install_handler(signum: i32, handler: extern "C" fn(i32)) -> i32 {
    let act = SigAction {
        sa_handler: handler as usize,
        sa_flags: SA_RESTORER,
        sa_restorer: __oxide_sigreturn as *const () as usize,
        sa_mask: 0,
    };
    sigaction(signum, &act, core::ptr::null_mut())
}

/// Set a signal to be ignored
pub fn ignore_signal(signum: i32) -> i32 {
    let act = SigAction {
        sa_handler: SIG_IGN,
        sa_flags: 0,
        sa_restorer: 0,
        sa_mask: 0,
    };
    sigaction(signum, &act, core::ptr::null_mut())
}

/// Reset a signal to default handling
pub fn default_signal(signum: i32) -> i32 {
    let act = SigAction {
        sa_handler: SIG_DFL,
        sa_flags: 0,
        sa_restorer: 0,
        sa_mask: 0,
    };
    sigaction(signum, &act, core::ptr::null_mut())
}
