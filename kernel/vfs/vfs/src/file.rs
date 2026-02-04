//! File handle abstraction
//!
//! Represents an open file with position and flags.

use alloc::sync::Arc;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use bitflags::bitflags;

use crate::error::{VfsError, VfsResult};
use crate::vnode::VnodeOps;

bitflags! {
    /// File open flags
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct FileFlags: u32 {
        /// Open for reading
        const O_RDONLY = 0;
        /// Open for writing
        const O_WRONLY = 1;
        /// Open for reading and writing
        const O_RDWR = 2;
        /// Access mode mask
        const O_ACCMODE = 3;

        /// Create file if it doesn't exist
        const O_CREAT = 0o100;
        /// Fail if file exists (with O_CREAT)
        const O_EXCL = 0o200;
        /// Truncate file to zero length
        const O_TRUNC = 0o1000;
        /// Append mode
        const O_APPEND = 0o2000;
        /// Non-blocking mode
        const O_NONBLOCK = 0o4000;
        /// Directory (fail if not a directory)
        const O_DIRECTORY = 0o200000;
        /// Don't follow symlinks
        const O_NOFOLLOW = 0o400000;
        /// Close on exec
        const O_CLOEXEC = 0o2000000;
    }
}

impl FileFlags {
    /// Check if readable
    pub fn readable(&self) -> bool {
        let mode = self.bits() & Self::O_ACCMODE.bits();
        mode == Self::O_RDONLY.bits() || mode == Self::O_RDWR.bits()
    }

    /// Check if writable
    pub fn writable(&self) -> bool {
        let mode = self.bits() & Self::O_ACCMODE.bits();
        mode == Self::O_WRONLY.bits() || mode == Self::O_RDWR.bits()
    }
}

/// Seek origin
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeekFrom {
    /// Seek from start of file
    Start(u64),
    /// Seek from end of file
    End(i64),
    /// Seek from current position
    Current(i64),
}

/// An open file
pub struct File {
    /// The vnode this file refers to
    vnode: Arc<dyn VnodeOps>,
    /// Current file position
    position: AtomicU64,
    /// Open flags (mutable via fcntl F_SETFL)
    /// 🔥 GraveShift: Use AtomicU32 for thread-safe flag updates (fcntl support) 🔥
    flags: AtomicU32,
}

impl File {
    /// Create a new file handle
    pub fn new(vnode: Arc<dyn VnodeOps>, flags: FileFlags) -> Self {
        File {
            vnode,
            position: AtomicU64::new(0),
            flags: AtomicU32::new(flags.bits()),
        }
    }

    /// Get the vnode
    pub fn vnode(&self) -> &Arc<dyn VnodeOps> {
        &self.vnode
    }

    // — GraveShift: raw COM1 diag when read() hits a not-readable fd
    fn serial_debug_not_readable(flag_bits: u32) {
        fn write_byte(b: u8) {
            unsafe {
                let mut status: u8;
                loop {
                    core::arch::asm!("in al, dx", out("al") status, in("dx") 0x3FDu16, options(nomem, nostack));
                    if status & 0x20 != 0 { break; }
                }
                core::arch::asm!("out dx, al", in("al") b, in("dx") 0x3F8u16, options(nomem, nostack));
            }
        }
        fn write_str(s: &[u8]) { for &b in s { write_byte(b); } }
        fn write_hex(mut n: u32) {
            write_str(b"0x");
            if n == 0 { write_byte(b'0'); return; }
            let mut buf = [0u8; 8];
            let mut i = 0;
            while n > 0 { let d = (n & 0xF) as u8; buf[i] = if d < 10 { b'0' + d } else { b'a' + d - 10 }; n >>= 4; i += 1; }
            while i > 0 { i -= 1; write_byte(buf[i]); }
        }

        write_str(b"[RD:PERM] flags=");
        write_hex(flag_bits);
        write_str(b" accmode=");
        write_hex(flag_bits & 3);
        write_byte(b'\n');
    }

    /// Get the flags
    pub fn flags(&self) -> FileFlags {
        FileFlags::from_bits_truncate(self.flags.load(Ordering::Relaxed))
    }

    /// Set the flags (used by fcntl F_SETFL)
    /// 🔥 GraveShift: fcntl F_SETFL needs atomic flag updates 🔥
    pub fn set_flags(&self, flags: FileFlags) {
        self.flags.store(flags.bits(), Ordering::Relaxed);
    }

    /// Get current position
    pub fn position(&self) -> u64 {
        self.position.load(Ordering::Relaxed)
    }

    /// Set position
    pub fn set_position(&self, pos: u64) {
        self.position.store(pos, Ordering::Relaxed);
    }

    /// Read from file
    pub fn read(&self, buf: &mut [u8]) -> VfsResult<usize> {
        let flags = self.flags();
        if !flags.readable() {
            // — GraveShift: something opened this fd write-only and tried to read
            // Dump the raw flag bits to serial so we can see what went wrong
            Self::serial_debug_not_readable(flags.bits());
            return Err(VfsError::PermissionDenied);
        }

        let pos = self.position.load(Ordering::Relaxed);
        let n = self.vnode.read(pos, buf)?;
        self.position.fetch_add(n as u64, Ordering::Relaxed);
        Ok(n)
    }

    /// Write to file
    pub fn write(&self, buf: &[u8]) -> VfsResult<usize> {
        let flags = self.flags();
        if !flags.writable() {
            return Err(VfsError::PermissionDenied);
        }

        let pos = if flags.contains(FileFlags::O_APPEND) {
            self.vnode.size()
        } else {
            self.position.load(Ordering::Relaxed)
        };

        let n = self.vnode.write(pos, buf)?;
        self.position.store(pos + n as u64, Ordering::Relaxed);
        Ok(n)
    }

    /// Seek to position
    pub fn seek(&self, from: SeekFrom) -> VfsResult<u64> {
        let size = self.vnode.size();
        let current = self.position.load(Ordering::Relaxed);

        let new_pos = match from {
            SeekFrom::Start(pos) => pos,
            SeekFrom::End(offset) => {
                if offset < 0 {
                    size.checked_sub((-offset) as u64)
                        .ok_or(VfsError::InvalidArgument)?
                } else {
                    size.checked_add(offset as u64)
                        .ok_or(VfsError::InvalidArgument)?
                }
            }
            SeekFrom::Current(offset) => {
                if offset < 0 {
                    current
                        .checked_sub((-offset) as u64)
                        .ok_or(VfsError::InvalidArgument)?
                } else {
                    current
                        .checked_add(offset as u64)
                        .ok_or(VfsError::InvalidArgument)?
                }
            }
        };

        self.position.store(new_pos, Ordering::Relaxed);
        Ok(new_pos)
    }

    /// Get file statistics
    pub fn stat(&self) -> VfsResult<crate::vnode::Stat> {
        self.vnode.stat()
    }

    /// Truncate file
    pub fn truncate(&self, size: u64) -> VfsResult<()> {
        if !self.flags().writable() {
            return Err(VfsError::PermissionDenied);
        }
        self.vnode.truncate(size)
    }

    /// Perform device I/O control operation
    pub fn ioctl(&self, request: u64, arg: u64) -> VfsResult<i64> {
        self.vnode.ioctl(request, arg)
    }

    /// Check if file can be read from (for poll/select)
    ///
    /// Returns true if:
    /// - File is opened for reading AND
    /// - Data is available (vnode reports poll_read_ready)
    pub fn can_read(&self) -> bool {
        self.flags().readable() && self.vnode.poll_read_ready()
    }

    /// Check if file can be written to (for poll/select)
    ///
    /// Returns true if:
    /// - File is opened for writing AND
    /// - Write would not block (vnode reports poll_write_ready)
    pub fn can_write(&self) -> bool {
        self.flags().writable() && self.vnode.poll_write_ready()
    }
}
