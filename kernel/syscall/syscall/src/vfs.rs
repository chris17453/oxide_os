//! VFS-related system calls
//!
//! Provides open, close, read, write, lseek, stat, etc.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
// Re-export external vfs crate types for sibling modules (vfs_ext.rs)
// that can't directly `use vfs::` due to name collision with this module.
pub use vfs::mount::GLOBAL_VFS;
pub use vfs::{File, FileFlags, Mode, SeekFrom, VfsError, VnodeType};
pub use vfs::{epoll, eventfd, memfd};

use crate::errno;
use crate::socket;
use crate::{with_current_meta, with_current_meta_mut};

/// Maximum path length for syscalls
const MAX_PATH: usize = 4096;

/// Resolve a path against the current process's working directory
///
/// If path is absolute (starts with /), normalizes and returns it.
/// If relative, prepends the process's cwd and normalizes.
/// Handles . and .. path components.
pub fn resolve_path(path: &str) -> String {
    // Use unified model - get cwd from ProcessMeta
    let cwd = with_current_meta(|meta| meta.cwd.clone()).unwrap_or_else(|| String::from("/"));

    // Build full path
    let full_path = if path.starts_with('/') {
        String::from(path)
    } else if cwd == "/" {
        format!("/{}", path)
    } else {
        format!("{}/{}", cwd, path)
    };

    // Normalize the path: handle . and .. components
    normalize_path(&full_path)
}

/// Normalize a path by resolving . and .. components
fn normalize_path(path: &str) -> String {
    use alloc::vec::Vec;

    let mut components: Vec<&str> = Vec::new();

    for component in path.split('/') {
        match component {
            "" | "." => {
                // Skip empty components and current directory markers
            }
            ".." => {
                // Go up one directory (pop last component if any)
                components.pop();
            }
            name => {
                components.push(name);
            }
        }
    }

    // Build result path
    if components.is_empty() {
        String::from("/")
    } else {
        let mut result = String::new();
        for component in components {
            result.push('/');
            result.push_str(component);
        }
        result
    }
}

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

    // Get the path slice (caller must have done STAC)
    let path_bytes = unsafe { core::slice::from_raw_parts(path_ptr as *const u8, path_len) };

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
    // Enable access to user pages for SMAP (path string is in user space)
    unsafe {
        core::arch::asm!("stac", options(nomem, nostack));
    }

    let raw_path = match copy_path_from_user(path_ptr, path_len) {
        Some(p) => p,
        None => {
            unsafe {
                core::arch::asm!("clac", options(nomem, nostack));
            }
            return errno::EFAULT;
        }
    };

    // Resolve relative paths against cwd
    let path = resolve_path(raw_path);

    let flags = FileFlags::from_bits_truncate(flags);
    let mode = Mode::new(mode);

    // Try to look up the file
    let vnode = if flags.contains(FileFlags::O_CREAT) {
        // O_CREAT: create if not exists
        match GLOBAL_VFS.lookup(&path) {
            Ok(vnode) => {
                if flags.contains(FileFlags::O_EXCL) {
                    // O_EXCL with O_CREAT: fail if exists
                    unsafe {
                        core::arch::asm!("clac", options(nomem, nostack));
                    }
                    return errno::EEXIST;
                }
                vnode
            }
            Err(VfsError::NotFound) => {
                // Create the file
                match GLOBAL_VFS.lookup_parent(&path) {
                    Ok((parent, name)) => match parent.create(&name, mode) {
                        Ok(vnode) => vnode,
                        Err(e) => {
                            unsafe {
                                core::arch::asm!("clac", options(nomem, nostack));
                            }
                            return vfs_error_to_errno(e);
                        }
                    },
                    Err(e) => {
                        unsafe {
                            core::arch::asm!("clac", options(nomem, nostack));
                        }
                        return vfs_error_to_errno(e);
                    }
                }
            }
            Err(e) => {
                unsafe {
                    core::arch::asm!("clac", options(nomem, nostack));
                }
                return vfs_error_to_errno(e);
            }
        }
    } else {
        match GLOBAL_VFS.lookup(&path) {
            Ok(vnode) => vnode,
            Err(e) => {
                unsafe {
                    core::arch::asm!("clac", options(nomem, nostack));
                }
                return vfs_error_to_errno(e);
            }
        }
    };

    // Check O_DIRECTORY
    if flags.contains(FileFlags::O_DIRECTORY) && vnode.vtype() != VnodeType::Directory {
        unsafe {
            core::arch::asm!("clac", options(nomem, nostack));
        }
        return errno::ENOTDIR;
    }

    // Check if trying to write to directory
    if flags.writable() && vnode.vtype() == VnodeType::Directory {
        unsafe {
            core::arch::asm!("clac", options(nomem, nostack));
        }
        return errno::EISDIR;
    }

    // Truncate if O_TRUNC
    if flags.contains(FileFlags::O_TRUNC) && flags.writable() {
        if let Err(e) = vnode.truncate(0) {
            unsafe {
                core::arch::asm!("clac", options(nomem, nostack));
            }
            return vfs_error_to_errno(e);
        }
    }

    // Create file handle with mount reference counting
    // WireSaint: Track open files to prevent unmounting busy filesystems
    let file = if let Some(mount_ref) = GLOBAL_VFS.get_mount_ref_for_path(&path) {
        Arc::new(File::new_with_mount_ref(vnode, flags, mount_ref))
    } else {
        // No mount found (shouldn't happen, but fallback to basic File)
        Arc::new(File::new(vnode, flags))
    };

    // — GraveShift: Single with_current_meta_mut call — allocate fd while holding lock.
    // Previously split across two calls, releasing lock between check and alloc. Never again.
    let result = match with_current_meta_mut(|meta| meta.fd_table.alloc(file)) {
        Some(Ok(fd)) => fd as i64,
        Some(Err(e)) => vfs_error_to_errno(e),
        None => errno::ESRCH,
    };

    // Disable access to user pages
    unsafe {
        core::arch::asm!("clac", options(nomem, nostack));
    }

    result
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

    // Close fd using unified model
    match with_current_meta_mut(|meta| meta.fd_table.close(fd)) {
        Some(Ok(())) => 0,
        Some(Err(e)) => vfs_error_to_errno(e),
        None => errno::ESRCH,
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

    // Get file from fd table
    let file =
        match with_current_meta(|meta| meta.fd_table.get(fd).map(|fd_entry| fd_entry.file.clone()))
        {
            Some(Ok(f)) => f,
            Some(Err(e)) => return vfs_error_to_errno(e),
            None => return errno::EBADF,
        };

    // 🔥 O_NONBLOCK SUPPORT (Priority #6) 🔥
    // Check if this is a non-blocking read and data is NOT available
    if file.flags().contains(FileFlags::O_NONBLOCK) {
        // For non-blocking reads, check if data is available BEFORE blocking
        if !file.vnode().poll_read_ready() {
            // No data available and O_NONBLOCK set → return EAGAIN
            return errno::EAGAIN;
        }
    }

    // Enable kernel preemption for blocking reads (e.g., console input)
    // This allows timer interrupt to context switch us when blocked
    use core::ptr::addr_of;
    unsafe {
        if let Some(f) = (*addr_of!(super::SYSCALL_CONTEXT)).allow_kernel_preempt {
            f();
        }
    }

    // Read into user buffer (requires STAC/CLAC for SMAP)
    // Enable access to user pages
    unsafe {
        core::arch::asm!("stac", options(nomem, nostack));
    }

    let buffer = unsafe { core::slice::from_raw_parts_mut(buf as *mut u8, count) };

    let result = match file.read(buffer) {
        Ok(n) => n as i64,
        Err(e) => vfs_error_to_errno(e),
    };

    // Disable access to user pages
    unsafe {
        core::arch::asm!("clac", options(nomem, nostack));
    }

    // Disable kernel preemption after read completes
    unsafe {
        if let Some(f) = (*addr_of!(super::SYSCALL_CONTEXT)).disallow_kernel_preempt {
            f();
        }
    }

    result
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

    // Get file from fd table
    let file =
        match with_current_meta(|meta| meta.fd_table.get(fd).map(|fd_entry| fd_entry.file.clone()))
        {
            Some(Ok(f)) => f,
            Some(Err(e)) => return vfs_error_to_errno(e),
            None => return errno::EBADF,
        };

    // 🔥 O_NONBLOCK SUPPORT (Priority #6) 🔥
    // Check if this is a non-blocking write and buffer is full
    if file.flags().contains(FileFlags::O_NONBLOCK) {
        // For non-blocking writes, check if we can write BEFORE blocking
        if !file.vnode().poll_write_ready() {
            // Buffer full and O_NONBLOCK set → return EAGAIN
            return errno::EAGAIN;
        }
    }

    // — GraveShift: Allow kernel preemption during write. Without this, sys_write spins on
    // TERMINAL.lock() in non-preemptable kernel context. If the lock is held by a task that
    // was preempted mid-echo (kernel_preempt_ok was set for sys_read), the write spins forever
    // because the timer ISR can't context-switch back to the lock holder. Permanent deadlock.
    // Enabling preemption here lets the scheduler break the spin and return to the echo path.
    use core::ptr::addr_of;
    unsafe {
        if let Some(f) = (*addr_of!(super::SYSCALL_CONTEXT)).allow_kernel_preempt {
            f();
        }
    }

    // Get user buffer (requires STAC/CLAC for SMAP)
    // Enable access to user pages
    unsafe {
        core::arch::asm!("stac", options(nomem, nostack));
    }

    let buffer = unsafe { core::slice::from_raw_parts(buf as *const u8, count) };

    let result = match file.write(buffer) {
        Ok(n) => n as i64,
        Err(e) => vfs_error_to_errno(e),
    };

    // Disable access to user pages
    unsafe {
        core::arch::asm!("clac", options(nomem, nostack));
    }

    // Disallow kernel preemption once write is complete
    unsafe {
        if let Some(f) = (*addr_of!(super::SYSCALL_CONTEXT)).disallow_kernel_preempt {
            f();
        }
    }

    result
}

/// sys_lseek - Reposition file offset
///
/// # Arguments
/// * `fd` - File descriptor
/// * `offset` - Offset value
/// * `whence` - Reference point (0=SEEK_SET, 1=SEEK_CUR, 2=SEEK_END)
pub fn sys_lseek(fd: i32, offset: i64, whence: i32) -> i64 {
    // Get file using unified model
    let file =
        match with_current_meta(|meta| meta.fd_table.get(fd).map(|fd_entry| fd_entry.file.clone()))
        {
            Some(Ok(f)) => f,
            Some(Err(e)) => return vfs_error_to_errno(e),
            None => return errno::ESRCH,
        };

    let from = match whence {
        0 => SeekFrom::Start(offset as u64), // SEEK_SET
        1 => SeekFrom::Current(offset),      // SEEK_CUR
        2 => SeekFrom::End(offset),          // SEEK_END
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

    // Get file using unified model
    let file =
        match with_current_meta(|meta| meta.fd_table.get(fd).map(|fd_entry| fd_entry.file.clone()))
        {
            Some(Ok(f)) => f,
            Some(Err(e)) => return vfs_error_to_errno(e),
            None => return errno::ESRCH,
        };

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
    let raw_path = match copy_path_from_user(path_ptr, path_len) {
        Some(p) => p,
        None => return errno::EFAULT,
    };

    // Resolve relative paths against cwd
    let path = resolve_path(raw_path);

    if !validate_user_buffer(stat_buf, core::mem::size_of::<vfs::Stat>()) {
        return errno::EFAULT;
    }

    let vnode = match GLOBAL_VFS.lookup(&path) {
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

/// sys_lstat - Get file status by path, not following symlinks
///
/// Returns information about the symbolic link itself, not its target.
///
/// # Arguments
/// * `path_ptr` - Pointer to path string
/// * `path_len` - Length of path string
/// * `stat_buf` - Pointer to stat structure
pub fn sys_lstat(path_ptr: u64, path_len: usize, stat_buf: u64) -> i64 {
    let raw_path = match copy_path_from_user(path_ptr, path_len) {
        Some(p) => p,
        None => return errno::EFAULT,
    };

    // Resolve relative paths against cwd
    let path = resolve_path(raw_path);

    if !validate_user_buffer(stat_buf, core::mem::size_of::<vfs::Stat>()) {
        return errno::EFAULT;
    }

    // Use lookup which doesn't follow symlinks (returns the symlink vnode itself)
    let vnode = match GLOBAL_VFS.lookup(&path) {
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
    match with_current_meta_mut(|meta| meta.fd_table.dup(old_fd)) {
        Some(Ok(fd)) => fd as i64,
        Some(Err(e)) => vfs_error_to_errno(e),
        None => errno::ESRCH,
    }
}

/// sys_dup2 - Duplicate file descriptor to specific number
///
/// # Arguments
/// * `old_fd` - File descriptor to duplicate
/// * `new_fd` - Target file descriptor number
pub fn sys_dup2(old_fd: i32, new_fd: i32) -> i64 {
    match with_current_meta_mut(|meta| meta.fd_table.dup2(old_fd, new_fd)) {
        Some(Ok(fd)) => fd as i64,
        Some(Err(e)) => vfs_error_to_errno(e),
        None => errno::ESRCH,
    }
}

/// sys_ftruncate - Truncate file to specified length
///
/// # Arguments
/// * `fd` - File descriptor
/// * `length` - New file length
pub fn sys_ftruncate(fd: i32, length: u64) -> i64 {
    // Get file using unified model
    let file =
        match with_current_meta(|meta| meta.fd_table.get(fd).map(|fd_entry| fd_entry.file.clone()))
        {
            Some(Ok(f)) => f,
            Some(Err(e)) => return vfs_error_to_errno(e),
            None => return errno::ESRCH,
        };

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

    // Allocate fds using unified model
    let result = with_current_meta_mut(|meta| {
        // Allocate read fd
        let read_fd = match meta.fd_table.alloc(read_file) {
            Ok(fd) => fd,
            Err(e) => return Err(vfs_error_to_errno(e)),
        };

        // Allocate write fd
        let write_fd = match meta.fd_table.alloc(write_file) {
            Ok(fd) => fd,
            Err(_) => {
                // Failed to allocate write fd, close read fd
                let _ = meta.fd_table.close(read_fd);
                return Err(errno::EMFILE);
            }
        };

        Ok((read_fd, write_fd))
    });

    match result {
        Some(Ok((read_fd, write_fd))) => {
            // Write fds to user buffer
            unsafe {
                let pipefd = pipefd_ptr as *mut i32;
                *pipefd = read_fd;
                *pipefd.add(1) = write_fd;
            }
            0
        }
        Some(Err(e)) => e,
        None => errno::ESRCH,
    }
}

/// sys_ioctl - Device I/O control
///
/// # Arguments
/// * `fd` - File descriptor
/// * `request` - ioctl request code
/// * `arg` - ioctl argument (request-specific)
pub fn sys_ioctl(fd: i32, request: u64, arg: u64) -> i64 {
    // Get file using unified model
    let file =
        match with_current_meta(|meta| meta.fd_table.get(fd).map(|fd_entry| fd_entry.file.clone()))
        {
            Some(Ok(f)) => f,
            Some(Err(e)) => return vfs_error_to_errno(e),
            None => return errno::ESRCH,
        };

    // — SableWire: Allow kernel preemption during ioctl. tcsetattr() (TCSANOW) from curses
    // cbreak()/noecho() goes through here. The ioctl handler grabs tty/ldisc locks — if those
    // are held by a preempted task, we spin forever without preemption. Same deadlock pattern
    // as sys_write on TERMINAL.lock(). Let the scheduler break the spin.
    use core::ptr::addr_of;
    unsafe {
        if let Some(f) = (*addr_of!(super::SYSCALL_CONTEXT)).allow_kernel_preempt {
            f();
        }
    }

    // — GraveShift: STAC before ioctl — handlers like TIOCGWINSZ write to user pointers.
    // No STAC = SMAP violation = GPF. Every ioctl that touches arg as a pointer needs this.
    unsafe {
        core::arch::asm!("stac", options(nomem, nostack));
    }

    let result = match file.ioctl(request, arg) {
        Ok(result) => result,
        Err(e) => vfs_error_to_errno(e),
    };

    unsafe {
        core::arch::asm!("clac", options(nomem, nostack));
    }

    // — SableWire: Ioctl done, back to non-preemptable kernel context.
    unsafe {
        if let Some(f) = (*addr_of!(super::SYSCALL_CONTEXT)).disallow_kernel_preempt {
            f();
        }
    }

    result
}

/// sys_chmod - Change file mode bits
///
/// # Arguments
/// * `path_ptr` - Pointer to path string
/// * `path_len` - Length of path
/// * `mode` - New mode bits
pub fn sys_chmod(path_ptr: u64, path_len: usize, mode: u32) -> i64 {
    if !validate_user_buffer(path_ptr, path_len) {
        return errno::EFAULT;
    }

    if path_len == 0 || path_len > MAX_PATH {
        return errno::EINVAL;
    }

    // Enable access to user pages for SMAP
    unsafe {
        core::arch::asm!("stac", options(nomem, nostack));
    }

    let path_slice = unsafe { core::slice::from_raw_parts(path_ptr as *const u8, path_len) };

    let path_str = match core::str::from_utf8(path_slice) {
        Ok(s) => s,
        Err(_) => {
            unsafe {
                core::arch::asm!("clac", options(nomem, nostack));
            }
            return errno::EINVAL;
        }
    };

    let full_path = resolve_path(path_str);

    // Look up the vnode
    let result = match GLOBAL_VFS.lookup(&full_path) {
        Ok(vnode) => {
            // Try to set mode
            match vnode.chmod(mode) {
                Ok(()) => 0,
                Err(e) => vfs_error_to_errno(e),
            }
        }
        Err(e) => vfs_error_to_errno(e),
    };

    // Disable access to user pages
    unsafe {
        core::arch::asm!("clac", options(nomem, nostack));
    }

    result
}

/// sys_fchmod - Change file mode bits by file descriptor
///
/// # Arguments
/// * `fd` - File descriptor
/// * `mode` - New mode bits
pub fn sys_fchmod(fd: i32, mode: u32) -> i64 {
    // Get file using unified model
    let file =
        match with_current_meta(|meta| meta.fd_table.get(fd).map(|fd_entry| fd_entry.file.clone()))
        {
            Some(Ok(f)) => f,
            Some(Err(e)) => return vfs_error_to_errno(e),
            None => return errno::ESRCH,
        };

    // Get the vnode and set mode
    match file.vnode().chmod(mode) {
        Ok(()) => 0,
        Err(e) => vfs_error_to_errno(e),
    }
}

/// sys_chown - Change file owner and group
///
/// # Arguments
/// * `path_ptr` - Pointer to path string
/// * `path_len` - Length of path
/// * `uid` - New user ID (-1 to leave unchanged)
/// * `gid` - New group ID (-1 to leave unchanged)
pub fn sys_chown(path_ptr: u64, path_len: usize, uid: i32, gid: i32) -> i64 {
    if !validate_user_buffer(path_ptr, path_len) {
        return errno::EFAULT;
    }

    if path_len == 0 || path_len > MAX_PATH {
        return errno::EINVAL;
    }

    // Enable access to user pages for SMAP
    unsafe {
        core::arch::asm!("stac", options(nomem, nostack));
    }

    let path_slice = unsafe { core::slice::from_raw_parts(path_ptr as *const u8, path_len) };

    let path_str = match core::str::from_utf8(path_slice) {
        Ok(s) => s,
        Err(_) => {
            unsafe {
                core::arch::asm!("clac", options(nomem, nostack));
            }
            return errno::EINVAL;
        }
    };

    let full_path = resolve_path(path_str);

    // Look up the vnode
    let result = match GLOBAL_VFS.lookup(&full_path) {
        Ok(vnode) => {
            match vnode.chown(
                if uid >= 0 { Some(uid as u32) } else { None },
                if gid >= 0 { Some(gid as u32) } else { None },
            ) {
                Ok(()) => 0,
                Err(e) => vfs_error_to_errno(e),
            }
        }
        Err(e) => vfs_error_to_errno(e),
    };

    // Disable access to user pages
    unsafe {
        core::arch::asm!("clac", options(nomem, nostack));
    }

    result
}

/// sys_fchown - Change file owner and group by file descriptor
///
/// # Arguments
/// * `fd` - File descriptor
/// * `uid` - New user ID (-1 to leave unchanged)
/// * `gid` - New group ID (-1 to leave unchanged)
pub fn sys_fchown(fd: i32, uid: i32, gid: i32) -> i64 {
    // Get file using unified model
    let file =
        match with_current_meta(|meta| meta.fd_table.get(fd).map(|fd_entry| fd_entry.file.clone()))
        {
            Some(Ok(f)) => f,
            Some(Err(e)) => return vfs_error_to_errno(e),
            None => return errno::ESRCH,
        };

    match file.vnode().chown(
        if uid >= 0 { Some(uid as u32) } else { None },
        if gid >= 0 { Some(gid as u32) } else { None },
    ) {
        Ok(()) => 0,
        Err(e) => vfs_error_to_errno(e),
    }
}

/// Statfs structure for statfs/fstatfs syscalls
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Statfs {
    /// Filesystem type
    pub f_type: i64,
    /// Optimal transfer block size
    pub f_bsize: i64,
    /// Total data blocks in filesystem
    pub f_blocks: u64,
    /// Free blocks in filesystem
    pub f_bfree: u64,
    /// Free blocks available to unprivileged user
    pub f_bavail: u64,
    /// Total file nodes in filesystem
    pub f_files: u64,
    /// Free file nodes in filesystem
    pub f_ffree: u64,
    /// Filesystem ID
    pub f_fsid: [i32; 2],
    /// Maximum length of filenames
    pub f_namelen: i64,
    /// Fragment size
    pub f_frsize: i64,
    /// Mount flags
    pub f_flags: i64,
    /// Spare bytes
    pub f_spare: [i64; 4],
}

impl Statfs {
    /// Create a default Statfs structure
    pub const fn new() -> Self {
        Statfs {
            f_type: 0x4F584944, // "OXID" magic
            f_bsize: 4096,
            f_blocks: 0,
            f_bfree: 0,
            f_bavail: 0,
            f_files: 0,
            f_ffree: 0,
            f_fsid: [0, 0],
            f_namelen: 255,
            f_frsize: 4096,
            f_flags: 0,
            f_spare: [0; 4],
        }
    }
}

/// sys_statfs - Get filesystem statistics
///
/// # Arguments
/// * `path_ptr` - Path to any file within the mounted filesystem
/// * `path_len` - Length of path string
/// * `buf_ptr` - Pointer to Statfs structure in user space
///
/// # Returns
/// 0 on success, negative errno on error
pub fn sys_statfs(path_ptr: u64, path_len: usize, buf_ptr: usize) -> i64 {
    use crate::errno;

    if buf_ptr == 0 || buf_ptr >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    // Copy path from user space
    let raw_path = match copy_path_from_user(path_ptr, path_len) {
        Some(p) => p,
        None => return errno::EFAULT,
    };

    // Resolve relative paths against cwd
    let path = resolve_path(raw_path);

    // Get VFS stats for the path
    let statfs = if let Ok(info) = GLOBAL_VFS.statfs(&path) {
        Statfs {
            f_type: info.fs_type as i64,
            f_bsize: info.block_size as i64,
            f_blocks: info.total_blocks,
            f_bfree: info.free_blocks,
            f_bavail: info.available_blocks,
            f_files: info.total_inodes,
            f_ffree: info.free_inodes,
            f_fsid: [0, 0],
            f_namelen: info.max_name_len as i64,
            f_frsize: info.block_size as i64,
            f_flags: 0,
            f_spare: [0; 4],
        }
    } else {
        // Return defaults if path doesn't exist or no info available
        Statfs::new()
    };

    // Copy to userspace
    unsafe {
        core::arch::asm!("stac", options(nomem, nostack));
        let dest = buf_ptr as *mut Statfs;
        core::ptr::write_volatile(dest, statfs);
        core::arch::asm!("clac", options(nomem, nostack));
    }

    0
}

/// sys_fstatfs - Get filesystem statistics by file descriptor
///
/// # Arguments
/// * `fd` - File descriptor
/// * `buf_ptr` - Pointer to Statfs structure in user space
///
/// # Returns
/// 0 on success, negative errno on error
pub fn sys_fstatfs(fd: i32, buf_ptr: usize) -> i64 {
    use crate::errno;

    if buf_ptr == 0 || buf_ptr >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    // Verify the file descriptor is valid
    let fd_valid = match with_current_meta(|meta| meta.fd_table.get(fd).is_ok()) {
        Some(valid) => valid,
        None => return errno::ESRCH,
    };
    if !fd_valid {
        return errno::EBADF;
    }

    // For fstatfs, we return default filesystem stats
    // A proper implementation would track the file's path and look up its mount
    // For now, use root filesystem stats
    let statfs = if let Ok(info) = GLOBAL_VFS.statfs("/") {
        Statfs {
            f_type: info.fs_type as i64,
            f_bsize: info.block_size as i64,
            f_blocks: info.total_blocks,
            f_bfree: info.free_blocks,
            f_bavail: info.available_blocks,
            f_files: info.total_inodes,
            f_ffree: info.free_inodes,
            f_fsid: [0, 0],
            f_namelen: info.max_name_len as i64,
            f_frsize: info.block_size as i64,
            f_flags: 0,
            f_spare: [0; 4],
        }
    } else {
        Statfs::new()
    };

    // Copy to userspace
    unsafe {
        core::arch::asm!("stac", options(nomem, nostack));
        let dest = buf_ptr as *mut Statfs;
        core::ptr::write_volatile(dest, statfs);
        core::arch::asm!("clac", options(nomem, nostack));
    }

    0
}

// ============================================================================
// Mount syscalls
// ============================================================================

/// Mount flags (Linux-compatible values)
pub mod mount_flags {
    /// Read-only mount
    pub const MS_RDONLY: u32 = 1;
    /// Don't allow setuid/setgid
    pub const MS_NOSUID: u32 = 2;
    /// Don't interpret special files
    pub const MS_NODEV: u32 = 4;
    /// Don't allow program execution
    pub const MS_NOEXEC: u32 = 8;
    /// Writes are synced immediately
    pub const MS_SYNCHRONOUS: u32 = 16;
    /// Remount an existing mount
    pub const MS_REMOUNT: u32 = 32;
    /// Allow mandatory locks
    pub const MS_MANDLOCK: u32 = 64;
    /// Directory modifications are synchronous
    pub const MS_DIRSYNC: u32 = 128;
    /// Don't follow symlinks
    pub const MS_NOSYMFOLLOW: u32 = 256;
    /// Don't update access times
    pub const MS_NOATIME: u32 = 1024;
    /// Don't update directory access times
    pub const MS_NODIRATIME: u32 = 2048;
    /// Bind mount
    pub const MS_BIND: u32 = 4096;
    /// Move mount
    pub const MS_MOVE: u32 = 8192;
    /// Recursive mount
    pub const MS_REC: u32 = 16384;
    /// Silent flag
    pub const MS_SILENT: u32 = 32768;
    /// Relative atime updates
    pub const MS_RELATIME: u32 = 1 << 21;
    /// Strict atime updates
    pub const MS_STRICTATIME: u32 = 1 << 24;
    /// Make writes sync lazily
    pub const MS_LAZYTIME: u32 = 1 << 25;
}

/// sys_mount - Mount a filesystem
///
/// # Arguments
/// * `source_ptr` - Pointer to source device path (may be NULL for some fs types)
/// * `source_len` - Length of source path
/// * `target_ptr` - Pointer to mount point path
/// * `target_len` - Length of target path
/// * `fstype_ptr` - Pointer to filesystem type string
/// * `fstype_len` - Length of filesystem type
///
/// # Returns
/// 0 on success, negative errno on error
pub fn sys_mount(
    source_ptr: u64,
    source_len: usize,
    target_ptr: u64,
    target_len: usize,
    fstype_ptr: u64,
    fstype_len_and_flags: u64,
) -> i64 {
    use crate::SYSCALL_CONTEXT;
    use crate::errno;
    use core::ptr::addr_of;

    // Enable access to user pages for SMAP
    unsafe {
        core::arch::asm!("stac", options(nomem, nostack));
    }

    // Copy source path (may be null/empty for some filesystems like tmpfs)
    let source = if source_ptr != 0 && source_len > 0 {
        match copy_path_from_user(source_ptr, source_len) {
            Some(s) => s,
            None => {
                unsafe {
                    core::arch::asm!("clac", options(nomem, nostack));
                }
                return errno::EFAULT;
            }
        }
    } else {
        ""
    };

    // Copy target (mount point) path
    let target = match copy_path_from_user(target_ptr, target_len) {
        Some(t) => t,
        None => {
            unsafe {
                core::arch::asm!("clac", options(nomem, nostack));
            }
            return errno::EFAULT;
        }
    };

    // Copy filesystem type
    let fstype_len = (fstype_len_and_flags & 0xFFFF_FFFF) as usize;
    let flags = (fstype_len_and_flags >> 32) as u32;

    let fstype = match copy_path_from_user(fstype_ptr, fstype_len) {
        Some(f) => f,
        None => {
            unsafe {
                core::arch::asm!("clac", options(nomem, nostack));
            }
            return errno::EFAULT;
        }
    };

    // Resolve target path
    let mount_point = resolve_path(target);

    // Call the kernel mount callback
    let result = unsafe {
        let ctx = addr_of!(SYSCALL_CONTEXT);
        if let Some(mount_fn) = (*ctx).mount {
            mount_fn(source, &mount_point, fstype, flags)
        } else {
            errno::ENOSYS
        }
    };

    // Disable access to user pages
    unsafe {
        core::arch::asm!("clac", options(nomem, nostack));
    }

    result
}

/// sys_umount - Unmount a filesystem
///
/// # Arguments
/// * `target_ptr` - Pointer to mount point path
/// * `target_len` - Length of target path
/// * `flags` - Unmount flags
///
/// # Returns
/// 0 on success, negative errno on error
pub fn sys_umount(target_ptr: u64, target_len: usize, flags: u32) -> i64 {
    use crate::SYSCALL_CONTEXT;
    use crate::errno;
    use core::ptr::addr_of;

    // Copy target path
    let target = match copy_path_from_user(target_ptr, target_len) {
        Some(t) => t,
        None => return errno::EFAULT,
    };

    // Resolve path
    let mount_point = resolve_path(target);

    // Call the kernel umount callback
    unsafe {
        let ctx = addr_of!(SYSCALL_CONTEXT);
        if let Some(umount_fn) = (*ctx).umount {
            return umount_fn(&mount_point, flags);
        }
    }

    errno::ENOSYS
}

/// sys_pivot_root - Change the root filesystem
///
/// # Arguments
/// * `new_root_ptr` - Pointer to new root path string
/// * `new_root_len` - Length of new root path
/// * `put_old_ptr` - Pointer to put_old path string
/// * `put_old_len` - Length of put_old path
///
/// # Returns
/// 0 on success, negative errno on error
pub fn sys_pivot_root(
    new_root_ptr: u64,
    new_root_len: usize,
    put_old_ptr: u64,
    put_old_len: usize,
) -> i64 {
    use crate::SYSCALL_CONTEXT;
    use crate::errno;
    use core::ptr::addr_of;

    // Enable access to user pages for SMAP
    unsafe {
        core::arch::asm!("stac", options(nomem, nostack));
    }

    // Copy new_root path
    let new_root = match copy_path_from_user(new_root_ptr, new_root_len) {
        Some(s) => s,
        None => {
            unsafe {
                core::arch::asm!("clac", options(nomem, nostack));
            }
            return errno::EFAULT;
        }
    };

    // Copy put_old path
    let put_old = match copy_path_from_user(put_old_ptr, put_old_len) {
        Some(s) => s,
        None => {
            unsafe {
                core::arch::asm!("clac", options(nomem, nostack));
            }
            return errno::EFAULT;
        }
    };

    // Resolve paths
    let resolved_new_root = resolve_path(new_root);
    let resolved_put_old = resolve_path(put_old);

    // Call the kernel pivot_root callback
    let result = unsafe {
        let ctx = addr_of!(SYSCALL_CONTEXT);
        if let Some(pivot_fn) = (*ctx).pivot_root {
            pivot_fn(&resolved_new_root, &resolved_put_old)
        } else {
            errno::ENOSYS
        }
    };

    // Disable access to user pages
    unsafe {
        core::arch::asm!("clac", options(nomem, nostack));
    }

    result
}

/// fcntl - manipulate file descriptor
///
/// 🔥 PRIORITY #8 FIX - fcntl O_NONBLOCK handler for TTY 🔥
///
/// Implements F_GETFL and F_SETFL commands for managing file flags,
/// particularly O_NONBLOCK which vim uses for async operations.
pub fn sys_fcntl(fd: i32, cmd: i32, arg: u64) -> i64 {
    // fcntl command codes
    const F_GETFL: i32 = 3; // Get file status flags
    const F_SETFL: i32 = 4; // Set file status flags

    match cmd {
        F_GETFL => {
            // Get file flags
            with_current_meta(|meta| {
                if let Some(file) = meta.fd_table.get_file(fd) {
                    file.flags().bits() as i64
                } else {
                    errno::EBADF as i64
                }
            })
            .unwrap_or(errno::EBADF as i64)
        }

        F_SETFL => {
            // Set file flags
            // Only certain flags can be set: O_APPEND, O_NONBLOCK, O_ASYNC
            // 🔥 GraveShift: fcntl F_SETFL lets userspace change blocking mode 🔥
            let new_flags = arg as u32;
            let allowed_flags = FileFlags::O_APPEND.bits() | FileFlags::O_NONBLOCK.bits();
            let flags_to_set = new_flags & allowed_flags;

            with_current_meta_mut(|meta| {
                if let Some(file) = meta.fd_table.get_file(fd) {
                    // Get current flags and preserve access mode
                    let current_flags = file.flags();
                    let access_mode = current_flags.bits() & FileFlags::O_ACCMODE.bits();

                    // Create new flags: preserve access mode, update allowed flags
                    let new_combined = access_mode | flags_to_set;
                    let new_flags = FileFlags::from_bits_truncate(new_combined);

                    // Check if O_NONBLOCK changed - notify TTY if applicable
                    let old_nonblock = current_flags.contains(FileFlags::O_NONBLOCK);
                    let new_nonblock = (flags_to_set & FileFlags::O_NONBLOCK.bits()) != 0;

                    if old_nonblock != new_nonblock {
                        // Check if this is a TTY and notify it
                        let vnode = file.vnode();
                        if vnode.vtype() == VnodeType::CharDevice {
                            // Notify TTY via custom ioctl (TIOC_SET_NONBLOCK)
                            // TTY will check ioctl code and update internal nonblocking flag
                            // 🔥 GraveShift: Propagate errors instead of silent failure 🔥
                            const TIOC_SET_NONBLOCK: u64 = 0x5490;
                            if let Err(e) = vnode.ioctl(TIOC_SET_NONBLOCK, new_nonblock as u64) {
                                return vfs_error_to_errno(e);
                            }
                        }
                    }

                    // Update the file flags atomically
                    file.set_flags(new_flags);
                    0
                } else {
                    errno::EBADF as i64
                }
            })
            .unwrap_or(errno::EBADF as i64)
        }

        _ => {
            // Unsupported fcntl command
            errno::EINVAL as i64
        }
    }
}
