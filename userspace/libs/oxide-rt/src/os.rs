//! OS-level syscall wrappers — getcwd, chdir, getpid, and friends.
//!
//! — NeonRoot: The boring but essential plumbing. Every program needs
//! to know who it is and where it lives. Existential questions, syscall-style.

use crate::syscall::*;
use crate::nr;
use crate::types::UtsName;

/// getcwd — get current working directory
/// Writes the path into `buf`, returns bytes written or negative errno.
pub fn getcwd(buf: &mut [u8]) -> isize {
    syscall2(nr::GETCWD, buf.as_mut_ptr() as usize, buf.len()) as isize
}

/// chdir — change working directory
pub fn chdir(path: &[u8]) -> i32 {
    syscall2(nr::CHDIR, path.as_ptr() as usize, path.len()) as i32
}

/// getpid — get process ID
pub fn getpid() -> i32 {
    syscall0(nr::GETPID) as i32
}

/// getppid — get parent process ID
pub fn getppid() -> i32 {
    syscall0(nr::GETPPID) as i32
}

/// getuid — get user ID
pub fn getuid() -> u32 {
    syscall0(nr::GETUID) as u32
}

/// getgid — get group ID
pub fn getgid() -> u32 {
    syscall0(nr::GETGID) as u32
}

/// geteuid — get effective user ID
pub fn geteuid() -> u32 {
    syscall0(nr::GETEUID) as u32
}

/// getegid — get effective group ID
pub fn getegid() -> u32 {
    syscall0(nr::GETEGID) as u32
}

/// exit — terminate process with status code
pub fn exit(status: i32) -> ! {
    syscall_exit(status as usize)
}

/// exit_group — terminate all threads in process
pub fn exit_group(status: i32) -> ! {
    let _ = syscall1(nr::EXIT_GROUP, status as usize);
    // — NeonRoot: If exit_group somehow returns (it won't), exit the hard way
    syscall_exit(status as usize)
}

/// uname — get system information
pub fn uname(buf: &mut UtsName) -> i32 {
    syscall1(nr::UNAME, buf as *mut UtsName as usize) as i32
}

/// setsid — create a new session
pub fn setsid() -> i32 {
    syscall0(nr::SETSID) as i32
}

/// setpgid — set process group ID
pub fn setpgid(pid: i32, pgid: i32) -> i32 {
    syscall2(nr::SETPGID, pid as usize, pgid as usize) as i32
}

/// getpgid — get process group ID
pub fn getpgid(pid: i32) -> i32 {
    syscall1(nr::GETPGID, pid as usize) as i32
}

/// gettid — get thread ID
pub fn gettid() -> i32 {
    syscall0(nr::GETTID) as i32
}
