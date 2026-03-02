//! Process management syscalls — fork, exec, wait, kill.
//!
//! — BlackLatch: The lifecycle of a process: born from fork,
//! reincarnated by exec, waited upon by parents, killed by signals.
//! Just like real life, but with more segfaults.

use crate::syscall::*;
use crate::nr;

/// fork — create a child process
pub fn fork() -> i32 {
    syscall0(nr::FORK) as i32
}

/// execve — execute a program
/// OXIDE kernel expects: rdi=path_ptr, rsi=path_len, rdx=argv, r10=envp
pub fn execve(path: &[u8], argv: *const *const u8, envp: *const *const u8) -> i32 {
    syscall4(
        nr::EXECVE,
        path.as_ptr() as usize,
        path.len(),
        argv as usize,
        envp as usize,
    ) as i32
}

/// waitpid — wait for a specific child process
pub fn waitpid(pid: i32, status: &mut i32, options: i32) -> i32 {
    syscall3(
        nr::WAITPID,
        pid as usize,
        status as *mut i32 as usize,
        options as usize,
    ) as i32
}

/// wait — wait for any child process
pub fn wait(status: &mut i32) -> i32 {
    syscall1(nr::WAIT, status as *mut i32 as usize) as i32
}

/// kill — send a signal to a process
pub fn kill(pid: i32, sig: i32) -> i32 {
    syscall2(nr::KILL, pid as usize, sig as usize) as i32
}

/// abort — terminate abnormally (sends SIGABRT to self)
pub fn abort() -> ! {
    let _ = kill(crate::os::getpid(), 6); // SIGABRT = 6
    crate::os::exit(134) // 128 + 6
}

/// Extract exit status from waitpid status
pub fn wexitstatus(status: i32) -> i32 {
    (status >> 8) & 0xFF
}

/// Check if process exited normally
pub fn wifexited(status: i32) -> bool {
    (status & 0x7F) == 0
}

/// Check if process was killed by a signal
pub fn wifsignaled(status: i32) -> bool {
    (status & 0x7F) != 0 && (status & 0x7F) != 0x7F
}

/// Get the signal that killed the process
pub fn wtermsig(status: i32) -> i32 {
    status & 0x7F
}

/// Check if process is stopped
pub fn wifstopped(status: i32) -> bool {
    (status & 0xFF) == 0x7F
}

/// Get the signal that stopped the process
pub fn wstopsig(status: i32) -> i32 {
    (status >> 8) & 0xFF
}
