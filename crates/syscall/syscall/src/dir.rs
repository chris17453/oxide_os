//! Directory-related system calls
//!
//! Provides mkdir, rmdir, unlink, rename, getcwd, chdir, readdir

extern crate alloc;

/// Align value up to the given alignment
const fn align_up(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}

use alloc::string::String;
use vfs::{Mode, mount::GLOBAL_VFS};
use crate::with_current_meta;

use crate::copy_to_user;
use crate::errno;
use crate::vfs::{copy_path_from_user, resolve_path, validate_user_buffer, vfs_error_to_errno};

/// Copy a path from user space (internal helper)
fn get_path(path_ptr: u64, path_len: usize) -> Option<&'static str> {
    copy_path_from_user(path_ptr, path_len)
}

/// Get path and resolve against cwd
fn get_resolved_path(path_ptr: u64, path_len: usize) -> Option<String> {
    let raw = get_path(path_ptr, path_len)?;
    Some(resolve_path(raw))
}

/// sys_mkdir - Create a directory
///
/// # Arguments
/// * `path_ptr` - Pointer to path string
/// * `path_len` - Length of path string
/// * `mode` - Directory permissions
pub fn sys_mkdir(path_ptr: u64, path_len: usize, mode: u32) -> i64 {
    let path = match get_resolved_path(path_ptr, path_len) {
        Some(p) => p,
        None => return errno::EFAULT,
    };

    let mode = Mode::new(mode);

    // Get parent directory and name
    match GLOBAL_VFS.lookup_parent(&path) {
        Ok((parent, name)) => match parent.mkdir(&name, mode) {
            Ok(_) => 0,
            Err(e) => vfs_error_to_errno(e),
        },
        Err(e) => vfs_error_to_errno(e),
    }
}

/// sys_rmdir - Remove a directory
///
/// # Arguments
/// * `path_ptr` - Pointer to path string
/// * `path_len` - Length of path string
pub fn sys_rmdir(path_ptr: u64, path_len: usize) -> i64 {
    let path = match get_resolved_path(path_ptr, path_len) {
        Some(p) => p,
        None => return errno::EFAULT,
    };

    // Get parent directory and name
    match GLOBAL_VFS.lookup_parent(&path) {
        Ok((parent, name)) => match parent.rmdir(&name) {
            Ok(()) => 0,
            Err(e) => vfs_error_to_errno(e),
        },
        Err(e) => vfs_error_to_errno(e),
    }
}

/// sys_unlink - Remove a file
///
/// # Arguments
/// * `path_ptr` - Pointer to path string
/// * `path_len` - Length of path string
pub fn sys_unlink(path_ptr: u64, path_len: usize) -> i64 {
    let path = match get_resolved_path(path_ptr, path_len) {
        Some(p) => p,
        None => return errno::EFAULT,
    };

    // Get parent directory and name
    match GLOBAL_VFS.lookup_parent(&path) {
        Ok((parent, name)) => match parent.unlink(&name) {
            Ok(()) => 0,
            Err(e) => vfs_error_to_errno(e),
        },
        Err(e) => vfs_error_to_errno(e),
    }
}

/// sys_rename - Rename a file or directory
///
/// # Arguments
/// * `old_path_ptr` - Pointer to old path
/// * `old_path_len` - Length of old path
/// * `new_path_ptr` - Pointer to new path
/// * `new_path_len` - Length of new path
pub fn sys_rename(
    old_path_ptr: u64,
    old_path_len: usize,
    new_path_ptr: u64,
    new_path_len: usize,
) -> i64 {
    let old_path = match get_resolved_path(old_path_ptr, old_path_len) {
        Some(p) => p,
        None => return errno::EFAULT,
    };

    let new_path = match get_resolved_path(new_path_ptr, new_path_len) {
        Some(p) => p,
        None => return errno::EFAULT,
    };

    // Get old parent and name
    let (old_parent, old_name) = match GLOBAL_VFS.lookup_parent(&old_path) {
        Ok(r) => r,
        Err(e) => return vfs_error_to_errno(e),
    };

    // Get new parent and name
    let (new_parent, new_name) = match GLOBAL_VFS.lookup_parent(&new_path) {
        Ok(r) => r,
        Err(e) => return vfs_error_to_errno(e),
    };

    // Perform rename
    match old_parent.rename(&old_name, new_parent.as_ref(), &new_name) {
        Ok(()) => 0,
        Err(e) => vfs_error_to_errno(e),
    }
}

/// Directory entry for getdents syscall (user-space format)
///
/// Matches Linux's struct linux_dirent64 exactly.
/// The d_name field (null-terminated, variable length) follows in memory after d_type.
#[repr(C, packed)]
pub struct UserDirEntry {
    /// Inode number
    pub d_ino: u64,
    /// Offset to next entry
    pub d_off: u64,
    /// Length of this record
    pub d_reclen: u16,
    /// File type
    pub d_type: u8,
    // d_name follows immediately (no padding)
}

/// File type constants for d_type
pub mod d_type {
    pub const DT_UNKNOWN: u8 = 0;
    pub const DT_FIFO: u8 = 1;
    pub const DT_CHR: u8 = 2;
    pub const DT_DIR: u8 = 4;
    pub const DT_BLK: u8 = 6;
    pub const DT_REG: u8 = 8;
    pub const DT_LNK: u8 = 10;
    pub const DT_SOCK: u8 = 12;
}

use vfs::VnodeType;

fn vtype_to_dtype(vtype: VnodeType) -> u8 {
    match vtype {
        VnodeType::File => d_type::DT_REG,
        VnodeType::Directory => d_type::DT_DIR,
        VnodeType::Symlink => d_type::DT_LNK,
        VnodeType::CharDevice => d_type::DT_CHR,
        VnodeType::BlockDevice => d_type::DT_BLK,
        VnodeType::Fifo => d_type::DT_FIFO,
        VnodeType::Socket => d_type::DT_SOCK,
    }
}

/// sys_getdents - Get directory entries
///
/// # Arguments
/// * `fd` - File descriptor of open directory
/// * `buf` - User buffer for entries
/// * `count` - Size of buffer
pub fn sys_getdents(fd: i32, buf: u64, count: usize) -> i64 {
    if !validate_user_buffer(buf, count) {
        return errno::EFAULT;
    }

    let file = match with_current_meta(|meta| {
        meta.fd_table.get(fd).map(|fd_entry| fd_entry.file.clone())
    }) {
        Some(Ok(f)) => f,
        Some(Err(e)) => return vfs_error_to_errno(e),
        None => return errno::ESRCH,
    };

    // Check it's a directory
    if file.vnode().vtype() != VnodeType::Directory {
        return errno::ENOTDIR;
    }

    let mut offset = file.position();
    let mut bytes_written: usize = 0;

    loop {
        // Read next directory entry
        let entry = match file.vnode().readdir(offset) {
            Ok(Some(e)) => e,
            Ok(None) => break, // End of directory
            Err(e) => return vfs_error_to_errno(e),
        };

        // Calculate entry size (header + name + null terminator, aligned to 8)
        let name_len = entry.name.len();
        let reclen = align_up(core::mem::size_of::<UserDirEntry>() + name_len + 1, 8);

        // Check if entry fits in remaining buffer
        if bytes_written + reclen > count {
            break;
        }

        // Write entry to user buffer
        unsafe {
            let entry_ptr = buf + bytes_written as u64;

            // Prepare header
            let header = UserDirEntry {
                d_ino: entry.ino,
                d_off: offset + 1,
                d_reclen: reclen as u16,
                d_type: vtype_to_dtype(entry.file_type),
            };

            // Write header (packed struct, no padding)
            let header_bytes = core::slice::from_raw_parts(
                &header as *const UserDirEntry as *const u8,
                core::mem::size_of::<UserDirEntry>(),
            );
            if !copy_to_user(entry_ptr, header_bytes) {
                return errno::EFAULT;
            }

            // Write name immediately after header
            let name_ptr = entry_ptr + core::mem::size_of::<UserDirEntry>() as u64;
            if !copy_to_user(name_ptr, entry.name.as_bytes()) {
                return errno::EFAULT;
            }

            // Write null terminator
            let null_byte = [0u8];
            if !copy_to_user(name_ptr + name_len as u64, &null_byte) {
                return errno::EFAULT;
            }
        }

        bytes_written += reclen;
        offset += 1;
    }

    // Update file position
    file.set_position(offset);

    bytes_written as i64
}

/// sys_chdir - Change current working directory
///
/// # Arguments
/// * `path_ptr` - Pointer to path string
/// * `path_len` - Length of path string
pub fn sys_chdir(path_ptr: u64, path_len: usize) -> i64 {
    let path = match get_resolved_path(path_ptr, path_len) {
        Some(p) => p,
        None => return errno::EFAULT,
    };

    // Lookup the directory
    let vnode = match GLOBAL_VFS.lookup(&path) {
        Ok(v) => v,
        Err(e) => return vfs_error_to_errno(e),
    };

    // Verify it's a directory
    if vnode.vtype() != VnodeType::Directory {
        return errno::ENOTDIR;
    }

    // Update the process's current working directory
    match crate::with_current_meta_mut(|meta| {
        meta.cwd = path;
    }) {
        Some(()) => 0,
        None => errno::ESRCH,
    }
}

/// sys_getcwd - Get current working directory
///
/// # Arguments
/// * `buf` - User buffer for path
/// * `size` - Size of buffer
pub fn sys_getcwd(buf: u64, size: usize) -> i64 {
    if !validate_user_buffer(buf, size) {
        return errno::EFAULT;
    }

    // Get current working directory
    let cwd = match with_current_meta(|meta| meta.cwd.clone()) {
        Some(c) => c,
        None => return errno::ESRCH,
    };

    // Check buffer size
    if cwd.len() + 1 > size {
        return errno::ERANGE;
    }

    // Copy path to user buffer
    unsafe {
        if !copy_to_user(buf, cwd.as_bytes()) {
            return errno::EFAULT;
        }

        // Write null terminator
        let null_byte = [0u8];
        if !copy_to_user(buf + cwd.len() as u64, &null_byte) {
            return errno::EFAULT;
        }
    }

    cwd.len() as i64
}

/// sys_link - Create hard link
///
/// # Arguments
/// * `target_ptr` - Pointer to target path
/// * `target_len` - Length of target path
/// * `link_ptr` - Pointer to link path
/// * `link_len` - Length of link path
pub fn sys_link(target_ptr: u64, target_len: usize, link_ptr: u64, link_len: usize) -> i64 {
    let target_path = match get_resolved_path(target_ptr, target_len) {
        Some(p) => p,
        None => return errno::EFAULT,
    };

    let link_path = match get_resolved_path(link_ptr, link_len) {
        Some(p) => p,
        None => return errno::EFAULT,
    };

    // Lookup the target
    let target_vnode = match GLOBAL_VFS.lookup(&target_path) {
        Ok(v) => v,
        Err(e) => return vfs_error_to_errno(e),
    };

    // Get link parent directory and name
    let (link_parent, link_name) = match GLOBAL_VFS.lookup_parent(&link_path) {
        Ok(r) => r,
        Err(e) => return vfs_error_to_errno(e),
    };

    // Create the hard link
    match link_parent.link(&link_name, target_vnode.as_ref()) {
        Ok(()) => 0,
        Err(e) => vfs_error_to_errno(e),
    }
}

/// sys_symlink - Create symbolic link
///
/// # Arguments
/// * `target_ptr` - Pointer to target path (not resolved, stored as-is)
/// * `target_len` - Length of target path
/// * `link_ptr` - Pointer to link path
/// * `link_len` - Length of link path
pub fn sys_symlink(target_ptr: u64, target_len: usize, link_ptr: u64, link_len: usize) -> i64 {
    // For symlink, target is NOT resolved - it's stored as-is
    let target_path = match get_path(target_ptr, target_len) {
        Some(p) => p,
        None => return errno::EFAULT,
    };

    let link_path = match get_resolved_path(link_ptr, link_len) {
        Some(p) => p,
        None => return errno::EFAULT,
    };

    // Get link parent directory and name
    let (link_parent, link_name) = match GLOBAL_VFS.lookup_parent(&link_path) {
        Ok(r) => r,
        Err(e) => return vfs_error_to_errno(e),
    };

    // Create the symbolic link
    match link_parent.symlink(&link_name, target_path) {
        Ok(_) => 0, // Returns Arc<dyn VnodeOps> but we don't need it
        Err(e) => vfs_error_to_errno(e),
    }
}

/// sys_readlink - Read value of symbolic link
///
/// # Arguments
/// * `path_ptr` - Pointer to symlink path
/// * `path_len` - Length of symlink path
/// * `buf` - User buffer for target path
/// * `bufsize` - Size of buffer
pub fn sys_readlink(path_ptr: u64, path_len: usize, buf: u64, bufsize: usize) -> i64 {
    if !validate_user_buffer(buf, bufsize) {
        return errno::EFAULT;
    }

    let path = match get_resolved_path(path_ptr, path_len) {
        Some(p) => p,
        None => return errno::EFAULT,
    };

    // Lookup the symlink
    // Note: VFS lookup returns the symlink vnode itself for symlinks
    let vnode = match GLOBAL_VFS.lookup(&path) {
        Ok(v) => v,
        Err(e) => return vfs_error_to_errno(e),
    };

    // Verify it's a symlink
    if vnode.vtype() != VnodeType::Symlink {
        return errno::EINVAL;
    }

    // Read the symlink target
    let target = match vnode.readlink() {
        Ok(t) => t,
        Err(e) => return vfs_error_to_errno(e),
    };

    // Copy to user buffer (up to bufsize, no null terminator per POSIX)
    let copy_len = core::cmp::min(target.len(), bufsize);
    unsafe {
        if !copy_to_user(buf, &target.as_bytes()[..copy_len]) {
            return errno::EFAULT;
        }
    }

    copy_len as i64
}

/// sys_utimes - Set file access and modification times
///
/// # Arguments
/// * `path_ptr` - Pointer to file path
/// * `path_len` - Length of file path
/// * `atime_sec` - Access time in seconds since epoch (u64::MAX = don't change)
/// * `mtime_sec` - Modification time in seconds since epoch (u64::MAX = don't change)
pub fn sys_utimes(path_ptr: u64, path_len: usize, atime_sec: u64, mtime_sec: u64) -> i64 {
    let path = match get_resolved_path(path_ptr, path_len) {
        Some(p) => p,
        None => return errno::EFAULT,
    };

    // Lookup the file/directory
    let vnode = match GLOBAL_VFS.lookup(&path) {
        Ok(v) => v,
        Err(e) => return vfs_error_to_errno(e),
    };

    // Convert u64::MAX to None (= don't change)
    let atime = if atime_sec == u64::MAX {
        None
    } else {
        Some(atime_sec)
    };

    let mtime = if mtime_sec == u64::MAX {
        None
    } else {
        Some(mtime_sec)
    };

    // Set the times
    match vnode.set_times(atime, mtime) {
        Ok(()) => 0,
        Err(e) => vfs_error_to_errno(e),
    }
}
