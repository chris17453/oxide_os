//! System call interface
//!
//! Provides raw syscall wrappers for x86_64.

use core::arch::asm;

/// Syscall numbers (must match kernel)
pub mod nr {
    pub const EXIT: u64 = 0;
    pub const WRITE: u64 = 1;
    pub const READ: u64 = 2;
    pub const FORK: u64 = 3;
    pub const EXEC: u64 = 4;
    pub const WAIT: u64 = 5;
    pub const WAITPID: u64 = 6;
    pub const GETPID: u64 = 7;
    pub const GETPPID: u64 = 8;
    pub const SETPGID: u64 = 9;
    pub const GETPGID: u64 = 10;
    pub const SETSID: u64 = 11;
    pub const GETSID: u64 = 12;
    pub const OPEN: u64 = 20;
    pub const CLOSE: u64 = 21;
    pub const LSEEK: u64 = 22;
    pub const FSTAT: u64 = 23;
    pub const STAT: u64 = 24;
    pub const DUP: u64 = 25;
    pub const DUP2: u64 = 26;
    pub const FTRUNCATE: u64 = 27;
    pub const MKDIR: u64 = 30;
    pub const RMDIR: u64 = 31;
    pub const UNLINK: u64 = 32;
    pub const RENAME: u64 = 33;
    pub const GETDENTS: u64 = 34;
    pub const IOCTL: u64 = 40;
    pub const KILL: u64 = 50;
    pub const SIGACTION: u64 = 51;
    pub const SIGPROCMASK: u64 = 52;
    pub const SIGPENDING: u64 = 53;
    pub const SIGSUSPEND: u64 = 54;
    pub const PAUSE: u64 = 55;
    pub const SIGRETURN: u64 = 56;
}

/// Raw syscall with 0 arguments
#[inline(always)]
pub fn syscall0(nr: u64) -> i64 {
    let ret: i64;
    unsafe {
        asm!(
            "syscall",
            in("rax") nr,
            lateout("rax") ret,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack)
        );
    }
    ret
}

/// Raw syscall with 1 argument
#[inline(always)]
pub fn syscall1(nr: u64, arg1: u64) -> i64 {
    let ret: i64;
    unsafe {
        asm!(
            "syscall",
            in("rax") nr,
            in("rdi") arg1,
            lateout("rax") ret,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack)
        );
    }
    ret
}

/// Raw syscall with 2 arguments
#[inline(always)]
pub fn syscall2(nr: u64, arg1: u64, arg2: u64) -> i64 {
    let ret: i64;
    unsafe {
        asm!(
            "syscall",
            in("rax") nr,
            in("rdi") arg1,
            in("rsi") arg2,
            lateout("rax") ret,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack)
        );
    }
    ret
}

/// Raw syscall with 3 arguments
#[inline(always)]
pub fn syscall3(nr: u64, arg1: u64, arg2: u64, arg3: u64) -> i64 {
    let ret: i64;
    unsafe {
        asm!(
            "syscall",
            in("rax") nr,
            in("rdi") arg1,
            in("rsi") arg2,
            in("rdx") arg3,
            lateout("rax") ret,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack)
        );
    }
    ret
}

/// Raw syscall with 4 arguments
#[inline(always)]
pub fn syscall4(nr: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64) -> i64 {
    let ret: i64;
    unsafe {
        asm!(
            "syscall",
            in("rax") nr,
            in("rdi") arg1,
            in("rsi") arg2,
            in("rdx") arg3,
            in("r10") arg4,
            lateout("rax") ret,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack)
        );
    }
    ret
}

/// sys_exit - Terminate process
pub fn sys_exit(status: i32) -> ! {
    syscall1(nr::EXIT, status as u64);
    loop {}
}

/// sys_write - Write to file descriptor
pub fn sys_write(fd: i32, buf: &[u8]) -> isize {
    syscall3(nr::WRITE, fd as u64, buf.as_ptr() as u64, buf.len() as u64) as isize
}

/// sys_read - Read from file descriptor
pub fn sys_read(fd: i32, buf: &mut [u8]) -> isize {
    syscall3(nr::READ, fd as u64, buf.as_mut_ptr() as u64, buf.len() as u64) as isize
}

/// sys_open - Open file
pub fn sys_open(path: &str, flags: u32, mode: u32) -> i32 {
    syscall4(nr::OPEN, path.as_ptr() as u64, path.len() as u64, flags as u64, mode as u64) as i32
}

/// sys_close - Close file descriptor
pub fn sys_close(fd: i32) -> i32 {
    syscall1(nr::CLOSE, fd as u64) as i32
}

/// sys_fork - Create child process
pub fn sys_fork() -> i32 {
    syscall0(nr::FORK) as i32
}

/// sys_exec - Execute new program
pub fn sys_exec(path: &str) -> i32 {
    syscall2(nr::EXEC, path.as_ptr() as u64, path.len() as u64) as i32
}

/// sys_wait - Wait for any child
pub fn sys_wait(status: &mut i32) -> i32 {
    syscall1(nr::WAIT, status as *mut i32 as u64) as i32
}

/// sys_waitpid - Wait for specific child
pub fn sys_waitpid(pid: i32, status: &mut i32, options: i32) -> i32 {
    syscall3(nr::WAITPID, pid as u64, status as *mut i32 as u64, options as u64) as i32
}

/// sys_getpid - Get process ID
pub fn sys_getpid() -> i32 {
    syscall0(nr::GETPID) as i32
}

/// sys_getppid - Get parent process ID
pub fn sys_getppid() -> i32 {
    syscall0(nr::GETPPID) as i32
}

/// sys_kill - Send signal to process
pub fn sys_kill(pid: i32, sig: i32) -> i32 {
    syscall2(nr::KILL, pid as u64, sig as u64) as i32
}

/// sys_dup - Duplicate file descriptor
pub fn sys_dup(fd: i32) -> i32 {
    syscall1(nr::DUP, fd as u64) as i32
}

/// sys_dup2 - Duplicate file descriptor to specific fd
pub fn sys_dup2(oldfd: i32, newfd: i32) -> i32 {
    syscall2(nr::DUP2, oldfd as u64, newfd as u64) as i32
}

/// sys_mkdir - Create directory
pub fn sys_mkdir(path: &str, mode: u32) -> i32 {
    syscall3(nr::MKDIR, path.as_ptr() as u64, path.len() as u64, mode as u64) as i32
}

/// sys_rmdir - Remove directory
pub fn sys_rmdir(path: &str) -> i32 {
    syscall2(nr::RMDIR, path.as_ptr() as u64, path.len() as u64) as i32
}

/// sys_unlink - Remove file
pub fn sys_unlink(path: &str) -> i32 {
    syscall2(nr::UNLINK, path.as_ptr() as u64, path.len() as u64) as i32
}

/// sys_getdents - Read directory entries
pub fn sys_getdents(fd: i32, buf: &mut [u8]) -> i32 {
    syscall3(nr::GETDENTS, fd as u64, buf.as_mut_ptr() as u64, buf.len() as u64) as i32
}

/// sys_ioctl - Device control
pub fn sys_ioctl(fd: i32, request: u64, arg: u64) -> i32 {
    syscall3(nr::IOCTL, fd as u64, request, arg) as i32
}
