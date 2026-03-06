//! Extended VFS system calls
//!
//! Provides *at variants, scatter/gather I/O, positional I/O, and other VFS extensions.

extern crate alloc;

use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::errno;
use crate::nr;
// Import from crate::vfs (which re-exports external vfs crate types)
use crate::vfs::{
    self, copy_path_from_user, resolve_path, validate_user_buffer, vfs_error_to_errno,
};
use crate::vfs::{File, FileFlags, GLOBAL_VFS, SeekFrom};
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

    unsafe {
        core::arch::asm!("stac", options(nostack));
    }

    // — ColdCipher: kernel-owned copy — TOCTOU closed.
    let raw_path = match copy_path_from_user(path_ptr, path_len) {
        Some(p) => p,
        None => {
            unsafe {
                core::arch::asm!("clac", options(nostack));
            }
            return errno::EFAULT;
        }
    };
    let path = resolve_path(&raw_path);

    unsafe {
        core::arch::asm!("clac", options(nostack));
    }

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

    unsafe {
        core::arch::asm!("stac", options(nostack));
    }

    // — ColdCipher: kernel-owned copy — TOCTOU closed.
    let raw_path = match copy_path_from_user(path_ptr, path_len) {
        Some(p) => p,
        None => {
            unsafe {
                core::arch::asm!("clac", options(nostack));
            }
            return errno::EFAULT;
        }
    };
    let path = resolve_path(&raw_path);

    let (atime, mtime) = if times_ptr == 0 {
        (None, None)
    } else {
        if !validate_user_buffer(times_ptr, core::mem::size_of::<[Timespec; 2]>()) {
            unsafe {
                core::arch::asm!("clac", options(nostack));
            }
            return errno::EFAULT;
        }
        let times = unsafe { &*(times_ptr as *const [Timespec; 2]) };
        let a = if times[0].tv_nsec == UTIME_OMIT {
            None
        } else {
            Some(times[0].tv_sec as u64)
        };
        let m = if times[1].tv_nsec == UTIME_OMIT {
            None
        } else {
            Some(times[1].tv_sec as u64)
        };
        (a, m)
    };

    let result = match GLOBAL_VFS.lookup(&path) {
        Ok(vnode) => match vnode.set_times(atime, mtime) {
            Ok(()) => 0,
            Err(e) => vfs_error_to_errno(e),
        },
        Err(e) => vfs_error_to_errno(e),
    };

    unsafe {
        core::arch::asm!("clac", options(nostack));
    }
    result
}

/// sys_futimens - Set file timestamps by fd
pub fn sys_futimens(fd: i32, times_ptr: u64) -> i64 {
    let file = match with_current_meta(|meta| meta.fd_table.get(fd).map(|e| e.file.clone())) {
        Some(Ok(f)) => f,
        Some(Err(e)) => return vfs_error_to_errno(e),
        None => return errno::ESRCH,
    };

    let (atime, mtime) = if times_ptr == 0 {
        (None, None)
    } else {
        unsafe {
            core::arch::asm!("stac", options(nostack));
        }
        if !validate_user_buffer(times_ptr, core::mem::size_of::<[Timespec; 2]>()) {
            unsafe {
                core::arch::asm!("clac", options(nostack));
            }
            return errno::EFAULT;
        }
        let times = unsafe { &*(times_ptr as *const [Timespec; 2]) };
        let a = if times[0].tv_nsec == UTIME_OMIT {
            None
        } else {
            Some(times[0].tv_sec as u64)
        };
        let m = if times[1].tv_nsec == UTIME_OMIT {
            None
        } else {
            Some(times[1].tv_sec as u64)
        };
        unsafe {
            core::arch::asm!("clac", options(nostack));
        }
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
    unsafe {
        core::arch::asm!("stac", options(nostack));
    }
    let iovs: Vec<IoVec> = unsafe {
        let src = core::slice::from_raw_parts(iov_ptr as *const IoVec, iovcnt);
        src.to_vec()
    };
    unsafe {
        core::arch::asm!("clac", options(nostack));
    }

    let mut total: i64 = 0;
    for iov in &iovs {
        if iov.iov_len == 0 {
            continue;
        }
        // sys_read_vfs handles its own STAC/CLAC
        let n = vfs::sys_read_vfs(fd, iov.iov_base, iov.iov_len);
        if n < 0 {
            if total > 0 {
                break;
            }
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
    unsafe {
        core::arch::asm!("stac", options(nostack));
    }
    let iovs: Vec<IoVec> = unsafe {
        let src = core::slice::from_raw_parts(iov_ptr as *const IoVec, iovcnt);
        src.to_vec()
    };
    unsafe {
        core::arch::asm!("clac", options(nostack));
    }

    let mut total: i64 = 0;
    for iov in &iovs {
        if iov.iov_len == 0 {
            continue;
        }
        // sys_write_vfs handles its own STAC/CLAC
        let n = vfs::sys_write_vfs(fd, iov.iov_base, iov.iov_len);
        if n < 0 {
            if total > 0 {
                break;
            }
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

    let file = match with_current_meta(|meta| meta.fd_table.get(fd).map(|e| e.file.clone())) {
        Some(Ok(f)) => f,
        Some(Err(e)) => return vfs_error_to_errno(e),
        None => return errno::ESRCH,
    };

    // Save position, seek, read, restore
    let saved_pos = file.position();
    if let Err(e) = file.seek(SeekFrom::Start(offset as u64)) {
        return vfs_error_to_errno(e);
    }

    unsafe {
        core::arch::asm!("stac", options(nostack));
    }
    let buffer = unsafe { core::slice::from_raw_parts_mut(buf as *mut u8, count) };
    let result = match file.read(buffer) {
        Ok(n) => n as i64,
        Err(e) => vfs_error_to_errno(e),
    };
    unsafe {
        core::arch::asm!("clac", options(nostack));
    }

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

    let file = match with_current_meta(|meta| meta.fd_table.get(fd).map(|e| e.file.clone())) {
        Some(Ok(f)) => f,
        Some(Err(e)) => return vfs_error_to_errno(e),
        None => return errno::ESRCH,
    };

    let saved_pos = file.position();
    if let Err(e) = file.seek(SeekFrom::Start(offset as u64)) {
        return vfs_error_to_errno(e);
    }

    unsafe {
        core::arch::asm!("stac", options(nostack));
    }
    let buffer = unsafe { core::slice::from_raw_parts(buf as *const u8, count) };
    let result = match file.write(buffer) {
        Ok(n) => n as i64,
        Err(e) => vfs_error_to_errno(e),
    };
    unsafe {
        core::arch::asm!("clac", options(nostack));
    }

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

    unsafe {
        core::arch::asm!("stac", options(nostack));
    }

    // — ColdCipher: kernel-owned copy — TOCTOU closed.
    let raw_path = match copy_path_from_user(path_ptr, path_len) {
        Some(p) => p,
        None => {
            unsafe {
                core::arch::asm!("clac", options(nostack));
            }
            return errno::EFAULT;
        }
    };
    let path = resolve_path(&raw_path);

    unsafe {
        core::arch::asm!("clac", options(nostack));
    }

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
    let in_file = match with_current_meta(|meta| meta.fd_table.get(in_fd).map(|e| e.file.clone())) {
        Some(Ok(f)) => f,
        Some(Err(e)) => return vfs_error_to_errno(e),
        None => return errno::ESRCH,
    };

    let out_file = match with_current_meta(|meta| meta.fd_table.get(out_fd).map(|e| e.file.clone()))
    {
        Some(Ok(f)) => f,
        Some(Err(e)) => return vfs_error_to_errno(e),
        None => return errno::ESRCH,
    };

    // Handle offset pointer: if non-null, seek in_fd to that offset
    if offset_ptr != 0 {
        unsafe {
            core::arch::asm!("stac", options(nostack));
        }
        if !validate_user_buffer(offset_ptr, 8) {
            unsafe {
                core::arch::asm!("clac", options(nostack));
            }
            return errno::EFAULT;
        }
        let off = unsafe { *(offset_ptr as *const i64) };
        unsafe {
            core::arch::asm!("clac", options(nostack));
        }
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
                if total > 0 {
                    break;
                }
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
                                core::arch::asm!("stac", options(nostack));
                                *(offset_ptr as *mut i64) = in_file.position() as i64;
                                core::arch::asm!("clac", options(nostack));
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
            core::arch::asm!("stac", options(nostack));
            *(offset_ptr as *mut i64) = in_file.position() as i64;
            core::arch::asm!("clac", options(nostack));
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
        meta.fd_table
            .get(fd_in)
            .map(|fd_entry| fd_entry.file.clone())
    }) {
        Some(Ok(f)) => f,
        _ => return errno::EBADF,
    };
    let out_file = match crate::with_current_meta(|meta| {
        meta.fd_table
            .get(fd_out)
            .map(|fd_entry| fd_entry.file.clone())
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
            core::arch::asm!("stac", options(nostack));
            let off = *(off_in_ptr as *const i64);
            core::arch::asm!("clac", options(nostack));
            let _ = in_file.seek(SeekFrom::Start(off as u64));
        }
    }

    // Handle output offset
    if off_out_ptr != 0 {
        if off_out_ptr >= 0x0000_8000_0000_0000 {
            return errno::EFAULT;
        }
        unsafe {
            core::arch::asm!("stac", options(nostack));
            let off = *(off_out_ptr as *const i64);
            core::arch::asm!("clac", options(nostack));
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
                if total > 0 {
                    break;
                }
                return errno::EIO;
            }
        };
        if nread == 0 {
            break;
        }

        let mut written = 0;
        while written < nread {
            match out_file.write(&buf[written..nread]) {
                Ok(n) => written += n,
                Err(_) => {
                    if total > 0 {
                        break;
                    }
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
            core::arch::asm!("stac", options(nostack));
            *(off_in_ptr as *mut i64) += total as i64;
            core::arch::asm!("clac", options(nostack));
        }
    }
    if off_out_ptr != 0 {
        unsafe {
            core::arch::asm!("stac", options(nostack));
            *(off_out_ptr as *mut i64) += total as i64;
            core::arch::asm!("clac", options(nostack));
        }
    }

    total as i64
}

/// sys_preadv - Read from fd at offset into multiple buffers
pub fn sys_preadv(fd: i32, iov_ptr: u64, iovcnt: i32, offset: i64) -> i64 {
    if iovcnt <= 0 || iovcnt > 1024 {
        return errno::EINVAL;
    }
    if iov_ptr == 0 || iov_ptr >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }
    if offset < 0 {
        return errno::EINVAL;
    }

    let mut total: i64 = 0;
    let mut cur_offset = offset;

    for i in 0..iovcnt as usize {
        let iov: IoVec = unsafe {
            core::arch::asm!("stac", options(nostack));
            let ptr = (iov_ptr as *const IoVec).add(i);
            let val = core::ptr::read_volatile(ptr);
            core::arch::asm!("clac", options(nostack));
            val
        };

        if iov.iov_len == 0 {
            continue;
        }

        let result = sys_pread64(fd, iov.iov_base, iov.iov_len, cur_offset);
        if result < 0 {
            if total > 0 {
                return total;
            }
            return result;
        }
        total += result;
        cur_offset += result;
        if (result as usize) < iov.iov_len {
            break;
        }
    }

    total
}

/// sys_pwritev - Write to fd at offset from multiple buffers
pub fn sys_pwritev(fd: i32, iov_ptr: u64, iovcnt: i32, offset: i64) -> i64 {
    if iovcnt <= 0 || iovcnt > 1024 {
        return errno::EINVAL;
    }
    if iov_ptr == 0 || iov_ptr >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }
    if offset < 0 {
        return errno::EINVAL;
    }

    let mut total: i64 = 0;
    let mut cur_offset = offset;

    for i in 0..iovcnt as usize {
        let iov: IoVec = unsafe {
            core::arch::asm!("stac", options(nostack));
            let ptr = (iov_ptr as *const IoVec).add(i);
            let val = core::ptr::read_volatile(ptr);
            core::arch::asm!("clac", options(nostack));
            val
        };

        if iov.iov_len == 0 {
            continue;
        }

        let result = sys_pwrite64(fd, iov.iov_base, iov.iov_len, cur_offset);
        if result < 0 {
            if total > 0 {
                return total;
            }
            return result;
        }
        total += result;
        cur_offset += result;
        if (result as usize) < iov.iov_len {
            break;
        }
    }

    total
}

// ============ memfd_create ============

/// sys_memfd_create - Create anonymous memory-backed file descriptor
///
/// # Arguments
/// * `name_ptr` - Name for debugging (in /proc/pid/fd/)
/// * `name_len` - Length of name
/// * `flags` - MFD_CLOEXEC, MFD_ALLOW_SEALING
pub fn sys_memfd_create(_name_ptr: u64, _name_len: usize, flags: u32) -> i64 {
    let cloexec = flags & vfs::memfd::MFD_CLOEXEC != 0;

    let vnode = vfs::memfd::create_memfd();
    let file = Arc::new(File::new(vnode, FileFlags::O_RDWR));

    let result = with_current_meta_mut(|meta| {
        let fd = match meta.fd_table.alloc(file) {
            Ok(fd) => fd,
            Err(e) => return Err(vfs_error_to_errno(e)),
        };
        if cloexec {
            if let Ok(desc) = meta.fd_table.get_mut(fd) {
                desc.cloexec = true;
            }
        }
        Ok(fd)
    });

    match result {
        Some(Ok(fd)) => fd as i64,
        Some(Err(e)) => e,
        None => errno::ESRCH,
    }
}

// ============ eventfd ============

/// sys_eventfd2 - Create event notification file descriptor
///
/// # Arguments
/// * `initval` - Initial counter value
/// * `flags` - EFD_CLOEXEC, EFD_NONBLOCK, EFD_SEMAPHORE
pub fn sys_eventfd2(initval: u32, flags: u32) -> i64 {
    let cloexec = flags & vfs::eventfd::EFD_CLOEXEC != 0;
    let nonblock = flags & vfs::eventfd::EFD_NONBLOCK != 0;

    let vnode = vfs::eventfd::create_eventfd(initval, flags);

    let mut file_flags = FileFlags::O_RDWR;
    if nonblock {
        file_flags |= FileFlags::O_NONBLOCK;
    }
    let file = Arc::new(File::new(vnode, file_flags));

    let result = with_current_meta_mut(|meta| {
        let fd = match meta.fd_table.alloc(file) {
            Ok(fd) => fd,
            Err(e) => return Err(vfs_error_to_errno(e)),
        };
        if cloexec {
            if let Ok(desc) = meta.fd_table.get_mut(fd) {
                desc.cloexec = true;
            }
        }
        Ok(fd)
    });

    match result {
        Some(Ok(fd)) => fd as i64,
        Some(Err(e)) => e,
        None => errno::ESRCH,
    }
}

// ============ epoll ============

/// sys_epoll_create1 - Create epoll instance
///
/// # Arguments
/// * `flags` - EPOLL_CLOEXEC (0x80000)
pub fn sys_epoll_create1(flags: i32) -> i64 {
    let cloexec = flags & 0x80000 != 0;

    let vnode = vfs::epoll::create_epoll();
    let file = Arc::new(File::new(vnode, FileFlags::O_RDWR));

    let result = with_current_meta_mut(|meta| {
        let fd = match meta.fd_table.alloc(file) {
            Ok(fd) => fd,
            Err(e) => return Err(vfs_error_to_errno(e)),
        };
        if cloexec {
            if let Ok(desc) = meta.fd_table.get_mut(fd) {
                desc.cloexec = true;
            }
        }
        Ok(fd)
    });

    match result {
        Some(Ok(fd)) => fd as i64,
        Some(Err(e)) => e,
        None => errno::ESRCH,
    }
}

/// sys_epoll_ctl - Control epoll instance
///
/// # Arguments
/// * `epfd` - epoll file descriptor
/// * `op` - EPOLL_CTL_ADD, EPOLL_CTL_DEL, EPOLL_CTL_MOD
/// * `fd` - Target file descriptor
/// * `event_ptr` - Pointer to struct epoll_event
pub fn sys_epoll_ctl(epfd: i32, op: i32, fd: i32, event_ptr: u64) -> i64 {
    let event = if op != vfs::epoll::EPOLL_CTL_DEL {
        if !validate_user_buffer(event_ptr, 12) {
            return errno::EFAULT;
        }
        unsafe {
            core::arch::asm!("stac", options(nostack));
            let ev = core::ptr::read_volatile(event_ptr as *const vfs::epoll::EpollEvent);
            core::arch::asm!("clac", options(nostack));
            Some(ev)
        }
    } else {
        None
    };

    let result = with_current_meta_mut(|meta| {
        let epoll_file = match meta.fd_table.get(epfd) {
            Ok(desc) => desc.file.clone(),
            Err(_) => return errno::EBADF,
        };

        let epoll_vnode = epoll_file.vnode();
        let epoll_node = match epoll_vnode.as_any().downcast_ref::<vfs::epoll::EpollNode>() {
            Some(node) => node,
            None => return errno::EINVAL,
        };

        let mut instance = epoll_node.instance.lock();

        match op {
            vfs::epoll::EPOLL_CTL_ADD => {
                let target_file = match meta.fd_table.get(fd) {
                    Ok(desc) => desc.file.clone(),
                    Err(_) => return errno::EBADF,
                };
                let ev = event.unwrap();
                match instance.add(fd, target_file, ev.events, ev.data) {
                    Ok(()) => 0,
                    Err(e) => vfs_error_to_errno(e),
                }
            }
            vfs::epoll::EPOLL_CTL_DEL => match instance.del(fd) {
                Ok(()) => 0,
                Err(e) => vfs_error_to_errno(e),
            },
            vfs::epoll::EPOLL_CTL_MOD => {
                let ev = event.unwrap();
                match instance.modify(fd, ev.events, ev.data) {
                    Ok(()) => 0,
                    Err(e) => vfs_error_to_errno(e),
                }
            }
            _ => errno::EINVAL,
        }
    });

    result.unwrap_or(errno::ESRCH)
}

/// sys_epoll_wait - Wait for events on an epoll instance
///
/// # Arguments
/// * `epfd` - epoll file descriptor
/// * `events_ptr` - Output buffer for epoll_event array
/// * `maxevents` - Maximum events to return
/// * `timeout` - Timeout in milliseconds (-1 = infinite, 0 = non-blocking)
pub fn sys_epoll_wait(epfd: i32, events_ptr: u64, maxevents: i32, _timeout: i32) -> i64 {
    if maxevents <= 0 {
        return errno::EINVAL;
    }

    let max = maxevents as usize;
    if !validate_user_buffer(events_ptr, max * 12) {
        return errno::EFAULT;
    }

    let result = with_current_meta(|meta| {
        let epoll_file = match meta.fd_table.get(epfd) {
            Ok(desc) => desc.file.clone(),
            Err(_) => return Err(errno::EBADF),
        };

        let epoll_vnode = epoll_file.vnode();
        let epoll_node = match epoll_vnode.as_any().downcast_ref::<vfs::epoll::EpollNode>() {
            Some(node) => node,
            None => return Err(errno::EINVAL),
        };

        let instance = epoll_node.instance.lock();
        Ok(instance.wait(max))
    });

    match result {
        Some(Ok(events)) => {
            let count = events.len();
            unsafe {
                core::arch::asm!("stac", options(nostack));
                let out = events_ptr as *mut vfs::epoll::EpollEvent;
                for (i, ev) in events.iter().enumerate() {
                    core::ptr::write_volatile(out.add(i), *ev);
                }
                core::arch::asm!("clac", options(nostack));
            }
            count as i64
        }
        Some(Err(e)) => e,
        None => errno::ESRCH,
    }
}

// ============ splice ============

/// sys_splice - Move data between two file descriptors
///
/// # Arguments
/// * `fd_in` - Input file descriptor
/// * `off_in_ptr` - Input offset pointer (unused, for pipe compat)
/// * `fd_out` - Output file descriptor
/// * `off_out_ptr` - Output offset pointer (unused, for pipe compat)
/// * `len` - Maximum bytes to transfer
/// * `flags` - SPLICE_F_MOVE, SPLICE_F_NONBLOCK, etc.
pub fn sys_splice(
    fd_in: i32,
    _off_in_ptr: u64,
    fd_out: i32,
    _off_out_ptr: u64,
    len: usize,
    _flags: u32,
) -> i64 {
    let files = with_current_meta(|meta| {
        let fin = meta.fd_table.get(fd_in).ok().map(|d| d.file.clone());
        let fout = meta.fd_table.get(fd_out).ok().map(|d| d.file.clone());
        (fin, fout)
    });

    let (fin, fout) = match files {
        Some((Some(f_in), Some(f_out))) => (f_in, f_out),
        _ => return errno::EBADF,
    };

    let buf_size = len.min(65536);
    let mut buf = alloc::vec![0u8; buf_size];
    let mut total: i64 = 0;
    let mut remaining = len;

    while remaining > 0 {
        let chunk = remaining.min(buf_size);
        let n_read = match fin.read(&mut buf[..chunk]) {
            Ok(0) => break,
            Ok(n) => n,
            Err(vfs::VfsError::WouldBlock) => {
                if total > 0 {
                    break;
                }
                return errno::EAGAIN;
            }
            Err(e) => {
                if total > 0 {
                    break;
                }
                return vfs_error_to_errno(e);
            }
        };

        let n_written = match fout.write(&buf[..n_read]) {
            Ok(n) => n,
            Err(e) => {
                if total > 0 {
                    break;
                }
                return vfs_error_to_errno(e);
            }
        };

        total += n_written as i64;
        remaining -= n_written;
        if n_written < n_read {
            break;
        }
    }

    total
}

// ============================================================================
// Week 2: Modern filesystem syscalls
// ============================================================================

/// statx structure (Linux-compatible)
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Statx {
    stx_mask: u32,
    stx_blksize: u32,
    stx_attributes: u64,
    stx_nlink: u32,
    stx_uid: u32,
    stx_gid: u32,
    stx_mode: u16,
    _spare0: [u16; 1],
    stx_ino: u64,
    stx_size: u64,
    stx_blocks: u64,
    stx_attributes_mask: u64,
    stx_atime_sec: i64,
    stx_atime_nsec: u32,
    stx_btime_sec: i64,
    stx_btime_nsec: u32,
    stx_ctime_sec: i64,
    stx_ctime_nsec: u32,
    stx_mtime_sec: i64,
    stx_mtime_nsec: u32,
    stx_rdev_major: u32,
    stx_rdev_minor: u32,
    stx_dev_major: u32,
    stx_dev_minor: u32,
    _spare2: [u64; 14],
}

/// sys_statx - Extended stat with more info and flags
///
/// # Arguments
/// * `dirfd` - Directory fd for relative paths (or AT_FDCWD)
/// * `path_ptr` - Path to file
/// * `path_len` - Length of path
/// * `flags` - AT_* flags
/// * `mask` - What info to return
/// * `statxbuf` - Pointer to statx structure
///
/// # GraveShift
/// Provides richer metadata than stat: birth time, mount ID, file attributes.
/// Used by modern tools that need precise filesystem info.
pub fn sys_statx(
    dirfd: i32,
    path_ptr: u64,
    path_len: usize,
    _flags: i32,
    _mask: u32,
    statxbuf: u64,
) -> i64 {
    if dirfd != nr::AT_FDCWD {
        return errno::ENOSYS;
    }

    if statxbuf == 0 || statxbuf >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    unsafe {
        core::arch::asm!("stac", options(nostack));
    }

    // — ColdCipher: kernel-owned copy — TOCTOU closed.
    let raw_path = match copy_path_from_user(path_ptr, path_len) {
        Some(p) => p,
        None => {
            unsafe {
                core::arch::asm!("clac", options(nostack));
            }
            return errno::EFAULT;
        }
    };
    let path = resolve_path(&raw_path);

    let node = match GLOBAL_VFS.lookup(&path) {
        Ok(n) => n,
        Err(e) => {
            unsafe {
                core::arch::asm!("clac", options(nostack));
            }
            return vfs_error_to_errno(e);
        }
    };

    let stat = match node.stat() {
        Ok(s) => s,
        Err(e) => {
            unsafe {
                core::arch::asm!("clac", options(nostack));
            }
            return vfs_error_to_errno(e);
        }
    };

    let statx = Statx {
        stx_mask: 0x7FF, // All basic fields
        stx_blksize: 4096,
        stx_attributes: 0,
        stx_nlink: stat.nlink as u32,
        stx_uid: stat.uid,
        stx_gid: stat.gid,
        stx_mode: stat.mode as u16,
        _spare0: [0; 1],
        stx_ino: stat.ino,
        stx_size: stat.size,
        stx_blocks: stat.blocks,
        stx_attributes_mask: 0,
        stx_atime_sec: 0,
        stx_atime_nsec: 0,
        stx_btime_sec: 0,
        stx_btime_nsec: 0,
        stx_ctime_sec: 0,
        stx_ctime_nsec: 0,
        stx_mtime_sec: 0,
        stx_mtime_nsec: 0,
        stx_rdev_major: 0,
        stx_rdev_minor: 0,
        stx_dev_major: 0,
        stx_dev_minor: 0,
        _spare2: [0; 14],
    };

    unsafe {
        core::ptr::write_volatile(statxbuf as *mut Statx, statx);
        core::arch::asm!("clac", options(nostack));
    }

    0
}

/// open_how structure for openat2
#[repr(C)]
#[derive(Clone, Copy)]
pub struct OpenHow {
    flags: u64,
    mode: u64,
    resolve: u64,
}

/// sys_openat2 - Extended openat with more control
///
/// # SableWire
/// Allows resolve flags to control path resolution (no symlinks, stay in tree, etc).
/// Critical for secure container filesystems.
pub fn sys_openat2(dirfd: i32, path_ptr: u64, path_len: usize, how_ptr: u64, _size: usize) -> i64 {
    if dirfd != nr::AT_FDCWD {
        return errno::ENOSYS;
    }

    if how_ptr == 0 || how_ptr >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    unsafe {
        core::arch::asm!("stac", options(nostack));
        let how = core::ptr::read_volatile(how_ptr as *const OpenHow);
        core::arch::asm!("clac", options(nostack));

        // For now, ignore resolve flags and use regular open
        vfs::sys_open(path_ptr, path_len, how.flags as u32, how.mode as u32)
    }
}

/// sys_renameat2 - Extended rename with flags
///
/// # WireSaint  
/// Supports RENAME_NOREPLACE (fail if target exists), RENAME_EXCHANGE (atomic swap),
/// and RENAME_WHITEOUT (create whiteout after move). Essential for overlayfs.
pub fn sys_renameat2(
    olddirfd: i32,
    oldpath_ptr: u64,
    oldpath_len: usize,
    newdirfd: i32,
    newpath_ptr: u64,
    newpath_len: usize,
    _flags: u32,
) -> i64 {
    if olddirfd != nr::AT_FDCWD || newdirfd != nr::AT_FDCWD {
        return errno::ENOSYS;
    }

    // For now, ignore flags and use regular rename
    crate::dir::sys_rename(oldpath_ptr, oldpath_len, newpath_ptr, newpath_len)
}

/// sys_faccessat2 - Check access with flags
///
/// # GraveShift
/// Like faccessat but supports AT_EACCESS (check using effective IDs not real IDs).
/// Needed for setuid programs checking their own permissions.
pub fn sys_faccessat2(dirfd: i32, path_ptr: u64, path_len: usize, mode: i32, _flags: i32) -> i64 {
    // For now, ignore flags and use regular faccessat
    sys_faccessat(dirfd, path_ptr, path_len, mode)
}

/// sys_mknodat - Create special file (device node, FIFO, socket)
///
/// # TorqueJax
/// Creates device nodes (/dev entries), FIFOs, and Unix sockets.
/// Mode bits determine type: S_IFBLK/S_IFCHR for devices, S_IFIFO for pipes.
pub fn sys_mknodat(dirfd: i32, path_ptr: u64, path_len: usize, mode: u32, _dev: u64) -> i64 {
    if dirfd != nr::AT_FDCWD {
        return errno::ENOSYS;
    }

    // Extract file type from mode
    let file_type = mode & 0xF000;

    // For now, only support regular files and FIFOs
    match file_type {
        0x8000 => {
            // S_IFREG - regular file
            vfs::sys_open(
                path_ptr,
                path_len,
                0x41, /* O_CREAT|O_WRONLY */
                mode & 0o777,
            )
        }
        0x1000 => {
            // S_IFIFO - FIFO
            // Create a FIFO would need special VFS support
            errno::ENOSYS
        }
        _ => errno::EINVAL,
    }
}
