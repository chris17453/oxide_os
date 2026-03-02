//! Filesystem syscall wrappers — where bytes meet inodes.
//!
//! — TorqueJax: OXIDE uses (ptr, len) pairs for paths, not null-terminated
//! C strings. The kernel reads the length from RSI. Remember this or
//! spend four hours wondering why your paths have garbage at the end.

use crate::syscall::*;
use crate::nr;
use crate::types::Stat;

/// Open a file. path is a byte slice (NOT null-terminated).
/// OXIDE kernel expects: rdi=path_ptr, rsi=path_len, rdx=flags, r10=mode
pub fn open(path: &[u8], flags: i32, mode: u32) -> i32 {
    syscall4(
        nr::OPEN,
        path.as_ptr() as usize,
        path.len(),
        flags as usize,
        mode as usize,
    ) as i32
}

/// stat — get file status by path
pub fn stat(path: &[u8], buf: &mut Stat) -> i32 {
    syscall3(
        nr::STAT,
        path.as_ptr() as usize,
        path.len(),
        buf as *mut Stat as usize,
    ) as i32
}

/// fstat — get file status by fd
pub fn fstat(fd: i32, buf: &mut Stat) -> i32 {
    syscall2(nr::FSTAT, fd as usize, buf as *mut Stat as usize) as i32
}

/// lstat — get symlink status (doesn't follow symlinks)
pub fn lstat(path: &[u8], buf: &mut Stat) -> i32 {
    syscall3(
        nr::LSTAT,
        path.as_ptr() as usize,
        path.len(),
        buf as *mut Stat as usize,
    ) as i32
}

/// lseek — reposition read/write offset
pub fn lseek(fd: i32, offset: i64, whence: i32) -> i64 {
    syscall3(nr::LSEEK, fd as usize, offset as usize, whence as usize) as i64
}

/// mkdir — create directory
pub fn mkdir(path: &[u8], mode: u32) -> i32 {
    syscall3(
        nr::MKDIR,
        path.as_ptr() as usize,
        path.len(),
        mode as usize,
    ) as i32
}

/// rmdir — remove directory
pub fn rmdir(path: &[u8]) -> i32 {
    syscall2(nr::RMDIR, path.as_ptr() as usize, path.len()) as i32
}

/// unlink — delete a file
pub fn unlink(path: &[u8]) -> i32 {
    syscall2(nr::UNLINK, path.as_ptr() as usize, path.len()) as i32
}

/// rename — rename a file or directory
pub fn rename(oldpath: &[u8], newpath: &[u8]) -> i32 {
    syscall4(
        nr::RENAME,
        oldpath.as_ptr() as usize,
        oldpath.len(),
        newpath.as_ptr() as usize,
        newpath.len(),
    ) as i32
}

/// readdir (getdents) — read directory entries
pub fn getdents(fd: i32, buf: &mut [u8]) -> i32 {
    syscall3(nr::GETDENTS, fd as usize, buf.as_mut_ptr() as usize, buf.len()) as i32
}

/// readlink — read value of a symbolic link
pub fn readlink(path: &[u8], buf: &mut [u8]) -> isize {
    syscall4(
        nr::READLINK,
        path.as_ptr() as usize,
        path.len(),
        buf.as_mut_ptr() as usize,
        buf.len(),
    ) as isize
}

/// symlink — create a symbolic link
pub fn symlink(target: &[u8], linkpath: &[u8]) -> i32 {
    syscall4(
        nr::SYMLINK,
        target.as_ptr() as usize,
        target.len(),
        linkpath.as_ptr() as usize,
        linkpath.len(),
    ) as i32
}

/// link — create a hard link
pub fn link(oldpath: &[u8], newpath: &[u8]) -> i32 {
    syscall4(
        nr::LINK,
        oldpath.as_ptr() as usize,
        oldpath.len(),
        newpath.as_ptr() as usize,
        newpath.len(),
    ) as i32
}

/// ftruncate — truncate a file to a specified length
pub fn ftruncate(fd: i32, length: i64) -> i32 {
    syscall2(nr::FTRUNCATE, fd as usize, length as usize) as i32
}

/// chmod — change file permissions
pub fn chmod(path: &[u8], mode: u32) -> i32 {
    syscall3(
        nr::CHMOD,
        path.as_ptr() as usize,
        path.len(),
        mode as usize,
    ) as i32
}

/// fchmod — change file permissions by fd
pub fn fchmod(fd: i32, mode: u32) -> i32 {
    syscall2(nr::FCHMOD, fd as usize, mode as usize) as i32
}

/// chown — change file owner/group
pub fn chown(path: &[u8], uid: u32, gid: u32) -> i32 {
    syscall4(
        nr::CHOWN,
        path.as_ptr() as usize,
        path.len(),
        uid as usize,
        gid as usize,
    ) as i32
}

/// fchown — change file owner/group by fd
pub fn fchown(fd: i32, uid: u32, gid: u32) -> i32 {
    syscall3(nr::FCHOWN, fd as usize, uid as usize, gid as usize) as i32
}

/// utimes — set file access and modification times
/// atime_sec/mtime_sec: seconds since epoch, u64::MAX = don't change
pub fn utimes(path: &[u8], atime_sec: u64, mtime_sec: u64) -> i32 {
    syscall4(
        nr::UTIMES,
        path.as_ptr() as usize,
        path.len(),
        atime_sec as usize,
        mtime_sec as usize,
    ) as i32
}

/// futimens — set file timestamps by fd
/// times_ptr: pointer to [Timespec; 2] (access, modification)
pub fn futimens(fd: i32, times: &[crate::types::Timespec; 2]) -> i32 {
    syscall2(nr::FUTIMENS, fd as usize, times.as_ptr() as usize) as i32
}

/// flock — advisory file locking
/// — ColdCipher: LOCK_SH=1, LOCK_EX=2, LOCK_NB=4, LOCK_UN=8.
/// Returns 0 on success, negative errno on failure.
pub fn flock(fd: i32, operation: i32) -> i32 {
    syscall2(nr::FLOCK, fd as usize, operation as usize) as i32
}
