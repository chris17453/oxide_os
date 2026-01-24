//! Directory operations

use crate::syscall;

/// Directory entry - parsed from raw bytes
pub struct Dirent {
    /// Inode number
    pub d_ino: u64,
    /// Offset to next entry
    pub d_off: i64,
    /// Length of this record
    pub d_reclen: u16,
    /// File type
    pub d_type: u8,
    /// Filename (null-terminated)
    pub d_name: [u8; 256],
}

impl Dirent {
    /// Parse from raw buffer at given offset
    fn from_bytes(buf: &[u8]) -> Self {
        let d_ino = u64::from_le_bytes([buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7]]);
        let d_off = i64::from_le_bytes([buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15]]);
        let d_reclen = u16::from_le_bytes([buf[16], buf[17]]);
        let d_type = buf[18];

        let mut d_name = [0u8; 256];
        let name_start = 19;
        let name_len = (d_reclen as usize).saturating_sub(name_start).min(255);
        d_name[..name_len].copy_from_slice(&buf[name_start..name_start + name_len]);

        Dirent { d_ino, d_off, d_reclen, d_type, d_name }
    }

    /// Get filename as str
    pub fn name(&self) -> &str {
        let len = self.d_name.iter().position(|&b| b == 0).unwrap_or(self.d_name.len());
        core::str::from_utf8(&self.d_name[..len]).unwrap_or("")
    }
}

/// Directory entry types
pub mod types {
    pub const DT_UNKNOWN: u8 = 0;
    pub const DT_FIFO: u8 = 1;
    pub const DT_CHR: u8 = 2;
    pub const DT_DIR: u8 = 4;
    pub const DT_BLK: u8 = 6;
    pub const DT_REG: u8 = 8;
    pub const DT_LNK: u8 = 10;
    pub const DT_SOCK: u8 = 12;
    pub const DT_WHT: u8 = 14;
}

/// Directory stream
pub struct Dir {
    fd: i32,
    buf: [u8; 4096],
    pos: usize,
    len: usize,
    /// Current parsed entry
    current: Dirent,
}

impl Dir {
    /// Create from file descriptor
    pub fn from_fd(fd: i32) -> Self {
        Dir {
            fd,
            buf: [0; 4096],
            pos: 0,
            len: 0,
            current: Dirent {
                d_ino: 0,
                d_off: 0,
                d_reclen: 0,
                d_type: 0,
                d_name: [0; 256],
            },
        }
    }
}

/// Open directory
pub fn opendir(path: &str) -> Option<Dir> {
    let fd = syscall::syscall4(
        syscall::SYS_OPEN,
        path.as_ptr() as usize,
        path.len(),
        0o200000, // O_DIRECTORY
        0,
    ) as i32;
    if fd < 0 { None } else { Some(Dir::from_fd(fd)) }
}

/// Read directory entry
pub fn readdir(dir: &mut Dir) -> Option<&Dirent> {
    // If buffer exhausted, read more
    if dir.pos >= dir.len {
        let ret = syscall::syscall3(
            syscall::SYS_GETDENTS64,
            dir.fd as usize,
            dir.buf.as_mut_ptr() as usize,
            dir.buf.len(),
        ) as isize;
        if ret <= 0 {
            return None;
        }
        dir.len = ret as usize;
        dir.pos = 0;
    }

    // Parse next entry
    if dir.pos < dir.len {
        dir.current = Dirent::from_bytes(&dir.buf[dir.pos..]);
        dir.pos += dir.current.d_reclen as usize;
        Some(&dir.current)
    } else {
        None
    }
}

/// Close directory
pub fn closedir(dir: Dir) -> i32 {
    syscall::syscall1(syscall::SYS_CLOSE, dir.fd as usize) as i32
}

/// Rewind directory to beginning
pub fn rewinddir(dir: &mut Dir) {
    syscall::syscall3(syscall::SYS_LSEEK, dir.fd as usize, 0, 0);
    dir.pos = 0;
    dir.len = 0;
}

/// Get current position in directory
pub fn telldir(dir: &Dir) -> i64 {
    syscall::syscall3(syscall::SYS_LSEEK, dir.fd as usize, 0, 1) as i64
}

/// Seek to position in directory
pub fn seekdir(dir: &mut Dir, pos: i64) {
    syscall::syscall3(syscall::SYS_LSEEK, dir.fd as usize, pos as usize, 0);
    dir.pos = 0;
    dir.len = 0;
}

/// Get directory file descriptor
pub fn dirfd(dir: &Dir) -> i32 {
    dir.fd
}
