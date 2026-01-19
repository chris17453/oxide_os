//! VFS-related system calls
//!
//! Provides open, close, read, write, lseek, stat, etc.

use alloc::sync::Arc;
use proc::process_table;
use vfs::{
    File, FileFlags, Mode, SeekFrom, VfsError, VnodeType,
    mount::GLOBAL_VFS,
};

use crate::errno;
use crate::socket;

/// Maximum path length for syscalls
const MAX_PATH: usize = 4096;

/// Copy a path from user space
///
/// Returns None if the path is invalid or too long.
pub fn copy_path_from_user(path_ptr: u64, path_len: usize) -> Option<&'static str> {
    // Validate pointer is in user space
    if path_ptr >= 0x0000_8000_0000_0000 {
        return None;
    }

    if path_len > MAX_PATH {
        return None;
    }

    if path_ptr.saturating_add(path_len as u64) >= 0x0000_8000_0000_0000 {
        return None;
    }

    // Get the path slice
    let path_bytes = unsafe {
        core::slice::from_raw_parts(path_ptr as *const u8, path_len)
    };

    core::str::from_utf8(path_bytes).ok()
}

/// Validate a user buffer
pub fn validate_user_buffer(buf: u64, len: usize) -> bool {
    if buf >= 0x0000_8000_0000_0000 {
        return false;
    }
    if buf.saturating_add(len as u64) >= 0x0000_8000_0000_0000 {
        return false;
    }
    true
}

/// Convert VfsError to errno
pub fn vfs_error_to_errno(e: VfsError) -> i64 {
    e.to_errno() as i64
}

/// sys_open - Open a file
///
/// # Arguments
/// * `path_ptr` - Pointer to path string
/// * `path_len` - Length of path string
/// * `flags` - Open flags
/// * `mode` - File creation mode (for O_CREAT)
pub fn sys_open(path_ptr: u64, path_len: usize, flags: u32, mode: u32) -> i64 {
    let path = match copy_path_from_user(path_ptr, path_len) {
        Some(p) => p,
        None => return errno::EFAULT,
    };

    let flags = FileFlags::from_bits_truncate(flags);
    let mode = Mode::new(mode);

    // Try to look up the file
    let vnode = if flags.contains(FileFlags::O_CREAT) {
        // O_CREAT: create if not exists
        match GLOBAL_VFS.lookup(path) {
            Ok(vnode) => {
                if flags.contains(FileFlags::O_EXCL) {
                    // O_EXCL with O_CREAT: fail if exists
                    return errno::EEXIST;
                }
                vnode
            }
            Err(VfsError::NotFound) => {
                // Create the file
                match GLOBAL_VFS.lookup_parent(path) {
                    Ok((parent, name)) => {
                        match parent.create(&name, mode) {
                            Ok(vnode) => vnode,
                            Err(e) => return vfs_error_to_errno(e),
                        }
                    }
                    Err(e) => return vfs_error_to_errno(e),
                }
            }
            Err(e) => return vfs_error_to_errno(e),
        }
    } else {
        match GLOBAL_VFS.lookup(path) {
            Ok(vnode) => vnode,
            Err(e) => return vfs_error_to_errno(e),
        }
    };

    // Check O_DIRECTORY
    if flags.contains(FileFlags::O_DIRECTORY) && vnode.vtype() != VnodeType::Directory {
        return errno::ENOTDIR;
    }

    // Check if trying to write to directory
    if flags.writable() && vnode.vtype() == VnodeType::Directory {
        return errno::EISDIR;
    }

    // Truncate if O_TRUNC
    if flags.contains(FileFlags::O_TRUNC) && flags.writable() {
        if let Err(e) = vnode.truncate(0) {
            return vfs_error_to_errno(e);
        }
    }

    // Create file handle
    let file = Arc::new(File::new(vnode, flags));

    // Get current process and allocate fd
    let table = process_table();
    let proc = match table.current() {
        Some(p) => p,
        None => return errno::ESRCH,
    };

    let mut proc_guard = proc.lock();
    match proc_guard.fd_table_mut().alloc(file) {
        Ok(fd) => fd as i64,
        Err(e) => vfs_error_to_errno(e),
    }
}

/// sys_close - Close a file descriptor
///
/// # Arguments
/// * `fd` - File descriptor to close
pub fn sys_close(fd: i32) -> i64 {
    // Check if this is a socket FD first
    if socket::is_socket_fd(fd) {
        return socket::close_socket(fd);
    }

    let table = process_table();
    let proc = match table.current() {
        Some(p) => p,
        None => return errno::ESRCH,
    };

    let mut proc_guard = proc.lock();
    match proc_guard.fd_table_mut().close(fd) {
        Ok(()) => 0,
        Err(e) => vfs_error_to_errno(e),
    }
}

/// sys_read_vfs - Read from a file descriptor using VFS
///
/// # Arguments
/// * `fd` - File descriptor
/// * `buf` - User buffer address
/// * `count` - Maximum number of bytes to read
pub fn sys_read_vfs(fd: i32, buf: u64, count: usize) -> i64 {
    if count == 0 {
        return 0;
    }

    if !validate_user_buffer(buf, count) {
        return errno::EFAULT;
    }

    let table = process_table();
    let proc = match table.current() {
        Some(p) => p,
        None => return errno::ESRCH,
    };

    let proc_guard = proc.lock();
    let file = match proc_guard.fd_table().get(fd) {
        Ok(fd_entry) => fd_entry.file.clone(),
        Err(e) => return vfs_error_to_errno(e),
    };
    drop(proc_guard);

    // Read into user buffer
    let buffer = unsafe {
        core::slice::from_raw_parts_mut(buf as *mut u8, count)
    };

    match file.read(buffer) {
        Ok(n) => n as i64,
        Err(e) => vfs_error_to_errno(e),
    }
}

/// sys_write_vfs - Write to a file descriptor using VFS
///
/// # Arguments
/// * `fd` - File descriptor
/// * `buf` - User buffer address
/// * `count` - Number of bytes to write
pub fn sys_write_vfs(fd: i32, buf: u64, count: usize) -> i64 {
    if count == 0 {
        return 0;
    }

    if !validate_user_buffer(buf, count) {
        return errno::EFAULT;
    }

    let table = process_table();
    let proc = match table.current() {
        Some(p) => p,
        None => return errno::ESRCH,
    };

    let proc_guard = proc.lock();
    let file = match proc_guard.fd_table().get(fd) {
        Ok(fd_entry) => fd_entry.file.clone(),
        Err(e) => return vfs_error_to_errno(e),
    };
    drop(proc_guard);

    // Get user buffer
    let buffer = unsafe {
        core::slice::from_raw_parts(buf as *const u8, count)
    };

    match file.write(buffer) {
        Ok(n) => n as i64,
        Err(e) => vfs_error_to_errno(e),
    }
}

/// sys_lseek - Reposition file offset
///
/// # Arguments
/// * `fd` - File descriptor
/// * `offset` - Offset value
/// * `whence` - Reference point (0=SEEK_SET, 1=SEEK_CUR, 2=SEEK_END)
pub fn sys_lseek(fd: i32, offset: i64, whence: i32) -> i64 {
    let table = process_table();
    let proc = match table.current() {
        Some(p) => p,
        None => return errno::ESRCH,
    };

    let proc_guard = proc.lock();
    let file = match proc_guard.fd_table().get(fd) {
        Ok(fd_entry) => fd_entry.file.clone(),
        Err(e) => return vfs_error_to_errno(e),
    };
    drop(proc_guard);

    let from = match whence {
        0 => SeekFrom::Start(offset as u64), // SEEK_SET
        1 => SeekFrom::Current(offset),       // SEEK_CUR
        2 => SeekFrom::End(offset),           // SEEK_END
        _ => return errno::EINVAL,
    };

    match file.seek(from) {
        Ok(pos) => pos as i64,
        Err(e) => vfs_error_to_errno(e),
    }
}

/// sys_fstat - Get file status by fd
///
/// # Arguments
/// * `fd` - File descriptor
/// * `stat_buf` - Pointer to stat structure
pub fn sys_fstat(fd: i32, stat_buf: u64) -> i64 {
    if !validate_user_buffer(stat_buf, core::mem::size_of::<vfs::Stat>()) {
        return errno::EFAULT;
    }

    let table = process_table();
    let proc = match table.current() {
        Some(p) => p,
        None => return errno::ESRCH,
    };

    let proc_guard = proc.lock();
    let file = match proc_guard.fd_table().get(fd) {
        Ok(fd_entry) => fd_entry.file.clone(),
        Err(e) => return vfs_error_to_errno(e),
    };
    drop(proc_guard);

    match file.stat() {
        Ok(stat) => {
            unsafe {
                let stat_ptr = stat_buf as *mut vfs::Stat;
                *stat_ptr = stat;
            }
            0
        }
        Err(e) => vfs_error_to_errno(e),
    }
}

/// sys_stat - Get file status by path
///
/// # Arguments
/// * `path_ptr` - Pointer to path string
/// * `path_len` - Length of path string
/// * `stat_buf` - Pointer to stat structure
pub fn sys_stat(path_ptr: u64, path_len: usize, stat_buf: u64) -> i64 {
    let path = match copy_path_from_user(path_ptr, path_len) {
        Some(p) => p,
        None => return errno::EFAULT,
    };

    if !validate_user_buffer(stat_buf, core::mem::size_of::<vfs::Stat>()) {
        return errno::EFAULT;
    }

    let vnode = match GLOBAL_VFS.lookup(path) {
        Ok(v) => v,
        Err(e) => return vfs_error_to_errno(e),
    };

    match vnode.stat() {
        Ok(stat) => {
            unsafe {
                let stat_ptr = stat_buf as *mut vfs::Stat;
                *stat_ptr = stat;
            }
            0
        }
        Err(e) => vfs_error_to_errno(e),
    }
}

/// sys_dup - Duplicate file descriptor
///
/// # Arguments
/// * `old_fd` - File descriptor to duplicate
pub fn sys_dup(old_fd: i32) -> i64 {
    let table = process_table();
    let proc = match table.current() {
        Some(p) => p,
        None => return errno::ESRCH,
    };

    let mut proc_guard = proc.lock();
    match proc_guard.fd_table_mut().dup(old_fd) {
        Ok(fd) => fd as i64,
        Err(e) => vfs_error_to_errno(e),
    }
}

/// sys_dup2 - Duplicate file descriptor to specific number
///
/// # Arguments
/// * `old_fd` - File descriptor to duplicate
/// * `new_fd` - Target file descriptor number
pub fn sys_dup2(old_fd: i32, new_fd: i32) -> i64 {
    let table = process_table();
    let proc = match table.current() {
        Some(p) => p,
        None => return errno::ESRCH,
    };

    let mut proc_guard = proc.lock();
    match proc_guard.fd_table_mut().dup2(old_fd, new_fd) {
        Ok(fd) => fd as i64,
        Err(e) => vfs_error_to_errno(e),
    }
}

/// sys_ftruncate - Truncate file to specified length
///
/// # Arguments
/// * `fd` - File descriptor
/// * `length` - New file length
pub fn sys_ftruncate(fd: i32, length: u64) -> i64 {
    let table = process_table();
    let proc = match table.current() {
        Some(p) => p,
        None => return errno::ESRCH,
    };

    let proc_guard = proc.lock();
    let file = match proc_guard.fd_table().get(fd) {
        Ok(fd_entry) => fd_entry.file.clone(),
        Err(e) => return vfs_error_to_errno(e),
    };
    drop(proc_guard);

    match file.truncate(length) {
        Ok(()) => 0,
        Err(e) => vfs_error_to_errno(e),
    }
}

/// sys_pipe - Create a pipe
///
/// # Arguments
/// * `pipefd_ptr` - Pointer to array of two i32 for read and write fds
pub fn sys_pipe(pipefd_ptr: u64) -> i64 {
    if !validate_user_buffer(pipefd_ptr, core::mem::size_of::<[i32; 2]>()) {
        return errno::EFAULT;
    }

    // Create pipe vnodes
    let (read_vnode, write_vnode) = match vfs::pipe::create_pipe() {
        Ok(pair) => pair,
        Err(e) => return vfs_error_to_errno(e),
    };

    // Create file handles for read and write ends
    let read_file = Arc::new(File::new(read_vnode, FileFlags::O_RDONLY));
    let write_file = Arc::new(File::new(write_vnode, FileFlags::O_WRONLY));

    // Get current process
    let table = process_table();
    let proc = match table.current() {
        Some(p) => p,
        None => return errno::ESRCH,
    };

    let mut proc_guard = proc.lock();
    let fd_table = proc_guard.fd_table_mut();

    // Allocate read fd
    let read_fd = match fd_table.alloc(read_file) {
        Ok(fd) => fd,
        Err(e) => return vfs_error_to_errno(e),
    };

    // Allocate write fd
    let write_fd = match fd_table.alloc(write_file) {
        Ok(fd) => fd,
        Err(_) => {
            // Failed to allocate write fd, close read fd
            let _ = fd_table.close(read_fd);
            return errno::EMFILE;
        }
    };

    // Write fds to user buffer
    unsafe {
        let pipefd = pipefd_ptr as *mut i32;
        *pipefd = read_fd;
        *pipefd.add(1) = write_fd;
    }

    0
}

/// sys_ioctl - Device I/O control
///
/// # Arguments
/// * `fd` - File descriptor
/// * `request` - ioctl request code
/// * `arg` - ioctl argument (request-specific)
pub fn sys_ioctl(fd: i32, request: u64, arg: u64) -> i64 {
    let table = process_table();
    let proc = match table.current() {
        Some(p) => p,
        None => return errno::ESRCH,
    };

    let proc_guard = proc.lock();
    let file = match proc_guard.fd_table().get(fd) {
        Ok(fd_entry) => fd_entry.file.clone(),
        Err(e) => return vfs_error_to_errno(e),
    };
    drop(proc_guard);

    // Call ioctl on the file
    match file.ioctl(request, arg) {
        Ok(result) => result,
        Err(e) => vfs_error_to_errno(e),
    }
}
