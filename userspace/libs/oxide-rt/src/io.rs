//! Basic I/O syscall wrappers — the bread and butter of everything.
//!
//! — SableWire: read, write, close. The holy trinity.
//! Everything else is just these three in a trench coat.

use crate::syscall::*;
use crate::nr;

/// Read from a file descriptor
pub fn read(fd: i32, buf: &mut [u8]) -> isize {
    syscall3(nr::READ, fd as usize, buf.as_mut_ptr() as usize, buf.len()) as isize
}

/// Write to a file descriptor
pub fn write(fd: i32, buf: &[u8]) -> isize {
    syscall3(nr::WRITE, fd as usize, buf.as_ptr() as usize, buf.len()) as isize
}

/// Close a file descriptor
pub fn close(fd: i32) -> i32 {
    syscall1(nr::CLOSE, fd as usize) as i32
}

/// Duplicate a file descriptor
pub fn dup(fd: i32) -> i32 {
    syscall1(nr::DUP, fd as usize) as i32
}

/// Duplicate a file descriptor to a specific number
pub fn dup2(oldfd: i32, newfd: i32) -> i32 {
    syscall2(nr::DUP2, oldfd as usize, newfd as usize) as i32
}

/// Duplicate with flags
pub fn dup3(oldfd: i32, newfd: i32, flags: i32) -> i32 {
    syscall3(nr::DUP3, oldfd as usize, newfd as usize, flags as usize) as i32
}

/// ioctl syscall
pub fn ioctl(fd: i32, request: u64, arg: usize) -> i32 {
    syscall3(nr::IOCTL, fd as usize, request as usize, arg) as i32
}

/// fcntl syscall
pub fn fcntl(fd: i32, cmd: i32, arg: usize) -> i32 {
    syscall3(nr::FCNTL, fd as usize, cmd as usize, arg) as i32
}

/// fsync — flush file data to disk
pub fn fsync(fd: i32) -> i32 {
    syscall1(nr::FSYNC, fd as usize) as i32
}

/// fdatasync — flush file data (not metadata) to disk
pub fn fdatasync(fd: i32) -> i32 {
    syscall1(nr::FDATASYNC, fd as usize) as i32
}
