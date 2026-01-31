//! Extended VFS system calls
//!
//! Provides *at variants, scatter/gather I/O, positional I/O, and other VFS extensions.

extern crate alloc;

use alloc::vec::Vec;

use crate::errno;
use crate::nr;
// Import from crate::vfs (which re-exports external vfs crate types)
use crate::vfs::{self, copy_path_from_user, resolve_path, validate_user_buffer, vfs_error_to_errno};
use crate::vfs::{SeekFrom, GLOBAL_VFS};
use crate::{with_current_meta, with_current_meta_mut};

/// IoVec structure for readv/writev (matches struct iovec)
#[repr(C)]
#[derive(Clone, Copy)]
struct IoVec {
    iov_base: u64,
    iov_len: usize,
}

/// Timespec for utimensat/futimens
#[repr(C)]
#[derive(Clone, Copy)]
struct Timespec {
    tv_sec: i64,
    tv_nsec: i64,
}

/// UTIME_OMIT: don't change this time
const UTIME_OMIT: i64 = (1 << 30) - 2;

// ============================================================================
// *at variants (operate relative to directory fd, or CWD when AT_FDCWD)
// ============================================================================

/// sys_openat - Open file relative to directory fd
pub fn sys_openat(dirfd: i32, path_ptr: u64, path_len: usize, flags: u32, mode: u32) -> i64 {
    if dirfd != nr::AT_FDCWD {
        return errno::ENOSYS;
    }
    vfs::sys_open(path_ptr, path_len, flags, mode)
}

/// sys_faccessat - Check file accessibility relative to directory fd
pub fn sys_faccessat(dirfd: i32, path_ptr: u64, path_len: usize, _mode: i32) -> i64 {
    if dirfd != nr::AT_FDCWD {
        return errno::ENOSYS;
    }

    unsafe { core::arch::asm!("stac", options(nomem, nostack)); }

    let raw_path = match copy_path_from_user(path_ptr, path_len) {
        Some(p) => p,
        None => {
            unsafe { core::arch::asm!("clac", options(nomem, nostack)); }
            return errno::EFAULT;
        }
    };
    let path = resolve_path(raw_path);

    unsafe { core::arch::asm!("clac", options(nomem, nostack)); }

    match GLOBAL_VFS.lookup(&path) {
        Ok(_) => 0,
        Err(e) => vfs_error_to_errno(e),
    }
}

/// sys_fchmodat - Change file mode relative to directory fd
pub fn sys_fchmodat(dirfd: i32, path_ptr: u64, path_len: usize, mode: u32) -> i64 {
    if dirfd != nr::AT_FDCWD {
        return errno::ENOSYS;
    }
    vfs::sys_chmod(path_ptr, path_len, mode)
}

/// sys_fchownat - Change file owner relative to directory fd
pub fn sys_fchownat(dirfd: i32, path_ptr: u64, path_len: usize, uid: i32, gid: i32) -> i64 {
    if dirfd != nr::AT_FDCWD {
        return errno::ENOSYS;
    }
    vfs::sys_chown(path_ptr, path_len, uid, gid)
}

/// sys_utimensat - Set file timestamps relative to directory fd
pub fn sys_utimensat(dirfd: i32, path_ptr: u64, path_len: usize, times_ptr: u64) -> i64 {
    if dirfd != nr::AT_FDCWD {
        return errno::ENOSYS;
    }

    unsafe { core::arch::asm!("stac", options(nomem, nostack)); }

    let raw_path = match copy_path_from_user(path_ptr, path_len) {
        Some(p) => p,
        None => {
            unsafe { core::arch::asm!("clac", options(nomem, nostack)); }
            return errno::EFAULT;
        }
    };
    let path = resolve_path(raw_path);

    let (atime, mtime) = if times_ptr == 0 {
        (None, None)
    } else {
        if !validate_user_buffer(times_ptr, core::mem::size_of::<[Timespec; 2]>()) {
            unsafe { core::arch::asm!("clac", options(nomem, nostack)); }
            return errno::EFAULT;
        }
        let times = unsafe { &*(times_ptr as *const [Timespec; 2]) };
        let a = if times[0].tv_nsec == UTIME_OMIT { None } else { Some(times[0].tv_sec as u64) };
        let m = if times[1].tv_nsec == UTIME_OMIT { None } else { Some(times[1].tv_sec as u64) };
        (a, m)
    };

    let result = match GLOBAL_VFS.lookup(&path) {
        Ok(vnode) => match vnode.set_times(atime, mtime) {
            Ok(()) => 0,
            Err(e) => vfs_error_to_errno(e),
        },
        Err(e) => vfs_error_to_errno(e),
    };

    unsafe { core::arch::asm!("clac", options(nomem, nostack)); }
    result
}

/// sys_futimens - Set file timestamps by fd
pub fn sys_futimens(fd: i32, times_ptr: u64) -> i64 {
    let file = match with_current_meta(|meta| {
        meta.fd_table.get(fd).map(|e| e.file.clone())
    }) {
        Some(Ok(f)) => f,
        Some(Err(e)) => return vfs_error_to_errno(e),
        None => return errno::ESRCH,
    };

    let (atime, mtime) = if times_ptr == 0 {
        (None, None)
    } else {
        unsafe { core::arch::asm!("stac", options(nomem, nostack)); }
        if !validate_user_buffer(times_ptr, core::mem::size_of::<[Timespec; 2]>()) {
            unsafe { core::arch::asm!("clac", options(nomem, nostack)); }
            return errno::EFAULT;
        }
        let times = unsafe { &*(times_ptr as *const [Timespec; 2]) };
        let a = if times[0].tv_nsec == UTIME_OMIT { None } else { Some(times[0].tv_sec as u64) };
        let m = if times[1].tv_nsec == UTIME_OMIT { None } else { Some(times[1].tv_sec as u64) };
        unsafe { core::arch::asm!("clac", options(nomem, nostack)); }
        (a, m)
    };

    match file.vnode().set_times(atime, mtime) {
        Ok(()) => 0,
        Err(e) => vfs_error_to_errno(e),
    }
}

// ============================================================================
// Scatter/Gather I/O
// ============================================================================

/// sys_readv - Read into multiple buffers
pub fn sys_readv(fd: i32, iov_ptr: u64, iovcnt: i32) -> i64 {
    if iovcnt <= 0 || iovcnt > 1024 {
        return if iovcnt == 0 { 0 } else { errno::EINVAL };
    }
    let iovcnt = iovcnt as usize;
    let iov_size = iovcnt * core::mem::size_of::<IoVec>();
    if !validate_user_buffer(iov_ptr, iov_size) {
        return errno::EFAULT;
    }

    // Copy IoVec array from user space to kernel to avoid STAC/CLAC nesting
    unsafe { core::arch::asm!("stac", options(nomem, nostack)); }
    let iovs: Vec<IoVec> = unsafe {
        let src = core::slice::from_raw_parts(iov_ptr as *const IoVec, iovcnt);
        src.to_vec()
    };
    unsafe { core::arch::asm!("clac", options(nomem, nostack)); }

    let mut total: i64 = 0;
    for iov in &iovs {
        if iov.iov_len == 0 {
            continue;
        }
        // sys_read_vfs handles its own STAC/CLAC
        let n = vfs::sys_read_vfs(fd, iov.iov_base, iov.iov_len);
        if n < 0 {
            if total > 0 { break; }
            return n;
        }
        total += n;
        if (n as usize) < iov.iov_len {
            break;
        }
    }
    total
}

/// sys_writev - Write from multiple buffers
pub fn sys_writev(fd: i32, iov_ptr: u64, iovcnt: i32) -> i64 {
    if iovcnt <= 0 || iovcnt > 1024 {
        return if iovcnt == 0 { 0 } else { errno::EINVAL };
    }
    let iovcnt = iovcnt as usize;
    let iov_size = iovcnt * core::mem::size_of::<IoVec>();
    if !validate_user_buffer(iov_ptr, iov_size) {
        return errno::EFAULT;
    }

    // Copy IoVec array from user space to kernel
    unsafe { core::arch::asm!("stac", options(nomem, nostack)); }
    let iovs: Vec<IoVec> = unsafe {
        let src = core::slice::from_raw_parts(iov_ptr as *const IoVec, iovcnt);
        src.to_vec()
    };
    unsafe { core::arch::asm!("clac", options(nomem, nostack)); }

    let mut total: i64 = 0;
    for iov in &iovs {
        if iov.iov_len == 0 {
            continue;
        }
        // sys_write_vfs handles its own STAC/CLAC
        let n = vfs::sys_write_vfs(fd, iov.iov_base, iov.iov_len);
        if n < 0 {
            if total > 0 { break; }
            return n;
        }
        total += n;
        if (n as usize) < iov.iov_len {
            break;
        }
    }
    total
}

// ============================================================================
// Positional I/O
// ============================================================================

/// sys_pread64 - Read from fd at given offset without changing file position
pub fn sys_pread64(fd: i32, buf: u64, count: usize, offset: i64) -> i64 {
    if offset < 0 {
        return errno::EINVAL;
    }
    if count == 0 {
        return 0;
    }
    if !validate_user_buffer(buf, count) {
        return errno::EFAULT;
    }

    let file = match with_current_meta(|meta| {
        meta.fd_table.get(fd).map(|e| e.file.clone())
    }) {
        Some(Ok(f)) => f,
        Some(Err(e)) => return vfs_error_to_errno(e),
        None => return errno::ESRCH,
    };

    // Save position, seek, read, restore
    let saved_pos = file.position();
    if let Err(e) = file.seek(SeekFrom::Start(offset as u64)) {
        return vfs_error_to_errno(e);
    }

    unsafe { core::arch::asm!("stac", options(nomem, nostack)); }
    let buffer = unsafe { core::slice::from_raw_parts_mut(buf as *mut u8, count) };
    let result = match file.read(buffer) {
        Ok(n) => n as i64,
        Err(e) => vfs_error_to_errno(e),
    };
    unsafe { core::arch::asm!("clac", options(nomem, nostack)); }

    // Restore original position
    file.set_position(saved_pos);
    result
}

/// sys_pwrite64 - Write to fd at given offset without changing file position
pub fn sys_pwrite64(fd: i32, buf: u64, count: usize, offset: i64) -> i64 {
    if offset < 0 {
        return errno::EINVAL;
    }
    if count == 0 {
        return 0;
    }
    if !validate_user_buffer(buf, count) {
        return errno::EFAULT;
    }

    let file = match with_current_meta(|meta| {
        meta.fd_table.get(fd).map(|e| e.file.clone())
    }) {
        Some(Ok(f)) => f,
        Some(Err(e)) => return vfs_error_to_errno(e),
        None => return errno::ESRCH,
    };

    let saved_pos = file.position();
    if let Err(e) = file.seek(SeekFrom::Start(offset as u64)) {
        return vfs_error_to_errno(e);
    }

    unsafe { core::arch::asm!("stac", options(nomem, nostack)); }
    let buffer = unsafe { core::slice::from_raw_parts(buf as *const u8, count) };
    let result = match file.write(buffer) {
        Ok(n) => n as i64,
        Err(e) => vfs_error_to_errno(e),
    };
    unsafe { core::arch::asm!("clac", options(nomem, nostack)); }

    file.set_position(saved_pos);
    result
}

// ============================================================================
// Descriptor management extensions
// ============================================================================

/// sys_dup3 - Duplicate fd with flags (O_CLOEXEC)
pub fn sys_dup3(old_fd: i32, new_fd: i32, _flags: i32) -> i64 {
    if old_fd == new_fd {
        return errno::EINVAL;
    }
    // Delegate to dup2 (O_CLOEXEC not meaningful without exec close-on-exec support yet)
    vfs::sys_dup2(old_fd, new_fd)
}

/// sys_pipe2 - Create pipe with flags
pub fn sys_pipe2(pipefd_ptr: u64, _flags: i32) -> i64 {
    // Delegate to pipe (flags like O_CLOEXEC/O_NONBLOCK not yet meaningful)
    vfs::sys_pipe(pipefd_ptr)
}

/// sys_truncate - Truncate file by path
pub fn sys_truncate(path_ptr: u64, path_len: usize, length: i64) -> i64 {
    if length < 0 {
        return errno::EINVAL;
    }

    unsafe { core::arch::asm!("stac", options(nomem, nostack)); }

    let raw_path = match copy_path_from_user(path_ptr, path_len) {
        Some(p) => p,
        None => {
            unsafe { core::arch::asm!("clac", options(nomem, nostack)); }
            return errno::EFAULT;
        }
    };
    let path = resolve_path(raw_path);

    unsafe { core::arch::asm!("clac", options(nomem, nostack)); }

    let vnode = match GLOBAL_VFS.lookup(&path) {
        Ok(v) => v,
        Err(e) => return vfs_error_to_errno(e),
    };

    match vnode.truncate(length as u64) {
        Ok(()) => 0,
        Err(e) => vfs_error_to_errno(e),
    }
}

/// sys_fsync - Synchronize file to disk
pub fn sys_fsync(fd: i32) -> i64 {
    // Verify fd is valid
    match with_current_meta(|meta| meta.fd_table.get(fd).is_ok()) {
        Some(true) => 0,
        Some(false) => errno::EBADF,
        None => errno::ESRCH,
    }
}

/// sys_fdatasync - Synchronize file data to disk
pub fn sys_fdatasync(fd: i32) -> i64 {
    sys_fsync(fd)
}

/// sys_sendfile - Copy data between file descriptors in kernel space
pub fn sys_sendfile(out_fd: i32, in_fd: i32, offset_ptr: u64, count: usize) -> i64 {
    let in_file = match with_current_meta(|meta| {
        meta.fd_table.get(in_fd).map(|e| e.file.clone())
    }) {
        Some(Ok(f)) => f,
        Some(Err(e)) => return vfs_error_to_errno(e),
        None => return errno::ESRCH,
    };

    let out_file = match with_current_meta(|meta| {
        meta.fd_table.get(out_fd).map(|e| e.file.clone())
    }) {
        Some(Ok(f)) => f,
        Some(Err(e)) => return vfs_error_to_errno(e),
        None => return errno::ESRCH,
    };

    // Handle offset pointer: if non-null, seek in_fd to that offset
    if offset_ptr != 0 {
        unsafe { core::arch::asm!("stac", options(nomem, nostack)); }
        if !validate_user_buffer(offset_ptr, 8) {
            unsafe { core::arch::asm!("clac", options(nomem, nostack)); }
            return errno::EFAULT;
        }
        let off = unsafe { *(offset_ptr as *const i64) };
        unsafe { core::arch::asm!("clac", options(nomem, nostack)); }
        if off < 0 {
            return errno::EINVAL;
        }
        if let Err(e) = in_file.seek(SeekFrom::Start(off as u64)) {
            return vfs_error_to_errno(e);
        }
    }

    let chunk_size = core::cmp::min(count, 8192);
    let mut buf = alloc::vec![0u8; chunk_size];
    let mut total = 0usize;

    while total < count {
        let to_read = core::cmp::min(count - total, chunk_size);
        let n = match in_file.read(&mut buf[..to_read]) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                if total > 0 { break; }
                return vfs_error_to_errno(e);
            }
        };

        let mut written = 0;
        while written < n {
            match out_file.write(&buf[written..n]) {
                Ok(w) => written += w,
                Err(e) => {
                    total += written;
                    if total > 0 {
                        // Update offset and return partial
                        if offset_ptr != 0 {
                            unsafe {
                                core::arch::asm!("stac", options(nomem, nostack));
                                *(offset_ptr as *mut i64) = in_file.position() as i64;
                                core::arch::asm!("clac", options(nomem, nostack));
                            }
                        }
                        return total as i64;
                    }
                    return vfs_error_to_errno(e);
                }
            }
        }
        total += n;
    }

    // Update offset pointer if provided
    if offset_ptr != 0 {
        unsafe {
            core::arch::asm!("stac", options(nomem, nostack));
            *(offset_ptr as *mut i64) = in_file.position() as i64;
            core::arch::asm!("clac", options(nomem, nostack));
        }
    }

    total as i64
}

/// sys_close_range - Close a range of file descriptors
pub fn sys_close_range(first: u32, last: u32, _flags: u32) -> i64 {
    if first > last {
        return errno::EINVAL;
    }

    match with_current_meta_mut(|meta| {
        for fd in first..=last {
            let _ = meta.fd_table.close(fd as i32);
        }
    }) {
        Some(()) => 0,
        None => errno::ESRCH,
    }
}

/// sys_copy_file_range - Copy data between file descriptors in kernel
pub fn sys_copy_file_range(
    fd_in: i32,
    off_in_ptr: u64,
    fd_out: i32,
    off_out_ptr: u64,
    len: usize,
    _flags: u32,
) -> i64 {
    if len == 0 {
        return 0;
    }

    // Get input and output files
    let in_file = match crate::with_current_meta(|meta| {
        meta.fd_table.get(fd_in).map(|fd_entry| fd_entry.file.clone())
    }) {
        Some(Ok(f)) => f,
        _ => return errno::EBADF,
    };
    let out_file = match crate::with_current_meta(|meta| {
        meta.fd_table.get(fd_out).map(|fd_entry| fd_entry.file.clone())
    }) {
        Some(Ok(f)) => f,
        _ => return errno::EBADF,
    };

    // Handle input offset
    if off_in_ptr != 0 {
        if off_in_ptr >= 0x0000_8000_0000_0000 {
            return errno::EFAULT;
        }
        unsafe {
            core::arch::asm!("stac", options(nomem, nostack));
            let off = *(off_in_ptr as *const i64);
            core::arch::asm!("clac", options(nomem, nostack));
            let _ = in_file.seek(SeekFrom::Start(off as u64));
        }
    }

    // Handle output offset
    if off_out_ptr != 0 {
        if off_out_ptr >= 0x0000_8000_0000_0000 {
            return errno::EFAULT;
        }
        unsafe {
            core::arch::asm!("stac", options(nomem, nostack));
            let off = *(off_out_ptr as *const i64);
            core::arch::asm!("clac", options(nomem, nostack));
            let _ = out_file.seek(SeekFrom::Start(off as u64));
        }
    }

    // Copy in chunks
    let mut buf = [0u8; 4096];
    let mut total: usize = 0;
    let mut remaining = len;

    while remaining > 0 {
        let chunk = core::cmp::min(remaining, buf.len());
        let nread = match in_file.read(&mut buf[..chunk]) {
            Ok(n) => n,
            Err(_) => {
                if total > 0 { break; }
                return errno::EIO;
            }
        };
        if nread == 0 { break; }

        let mut written = 0;
        while written < nread {
            match out_file.write(&buf[written..nread]) {
                Ok(n) => written += n,
                Err(_) => {
                    if total > 0 { break; }
                    return errno::EIO;
                }
            }
        }
        total += nread;
        remaining -= nread;
    }

    // Update offsets
    if off_in_ptr != 0 {
        unsafe {
            core::arch::asm!("stac", options(nomem, nostack));
            *(off_in_ptr as *mut i64) += total as i64;
            core::arch::asm!("clac", options(nomem, nostack));
        }
    }
    if off_out_ptr != 0 {
        unsafe {
            core::arch::asm!("stac", options(nomem, nostack));
            *(off_out_ptr as *mut i64) += total as i64;
            core::arch::asm!("clac", options(nomem, nostack));
        }
    }

    total as i64
}
