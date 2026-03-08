//! File handle abstraction
//!
//! Represents an open file with position and flags.

use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};

use bitflags::bitflags;

use crate::error::{VfsError, VfsResult};
use crate::flock::{self, InodeId};
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
        /// Don't set controlling terminal (for tty devices)
        const O_NOCTTY = 0o400;
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
    /// Reference to mount's open file counter (for unmount safety)
    /// WireSaint: Holds mount open while file is open, prevents premature unmount
    mount_ref_count: Option<Arc<AtomicUsize>>,
    /// — ColdCipher: Unique identity for advisory file locking.
    /// Each Arc<File> gets its own owner_id at birth. dup()/fork() share
    /// the Arc, so they share the lock. Last close releases it.
    owner_id: u64,
    /// — ColdCipher: Set to true when this file holds an advisory lock.
    /// Avoids stat()+spin::Mutex on every close for the 99.99% of files
    /// that never call flock(). SMP can't afford that tax.
    has_flock: AtomicBool,
}

impl File {
    /// Create a new file handle
    pub fn new(vnode: Arc<dyn VnodeOps>, flags: FileFlags) -> Self {
        File {
            vnode,
            position: AtomicU64::new(0),
            flags: AtomicU32::new(flags.bits()),
            mount_ref_count: None,
            owner_id: flock::next_owner_id(),
            has_flock: AtomicBool::new(false),
        }
    }

    /// Create a new file handle with mount reference counting
    /// WireSaint: Used when opening files to prevent unmounting while open
    pub fn new_with_mount_ref(
        vnode: Arc<dyn VnodeOps>,
        flags: FileFlags,
        mount_ref: Arc<AtomicUsize>,
    ) -> Self {
        // Increment the mount's open file counter
        mount_ref.fetch_add(1, Ordering::Relaxed);

        File {
            vnode,
            position: AtomicU64::new(0),
            flags: AtomicU32::new(flags.bits()),
            mount_ref_count: Some(mount_ref),
            owner_id: flock::next_owner_id(),
            has_flock: AtomicBool::new(false),
        }
    }

    /// Get the vnode
    pub fn vnode(&self) -> &Arc<dyn VnodeOps> {
        &self.vnode
    }

    /// — ColdCipher: Get the unique owner ID for advisory file locking.
    /// This is the identity used by FlockRegistry to track who holds what.
    pub fn owner_id(&self) -> u64 {
        self.owner_id
    }

    /// — ColdCipher: Mark this file as holding an advisory lock.
    /// Called by sys_flock when a lock is acquired. Enables Drop cleanup.
    pub fn mark_flock_held(&self) {
        self.has_flock.store(true, Ordering::Relaxed);
    }

    /// — ColdCipher: Clear the flock flag (on explicit LOCK_UN).
    pub fn clear_flock_held(&self) {
        self.has_flock.store(false, Ordering::Relaxed);
    }

    // — GraveShift: raw COM1 diag when read() hits a not-readable fd
    fn serial_debug_not_readable(flag_bits: u32) {
        // — SableWire: bounded spin — drop byte if UART FIFO is saturated.
        // raw asm purged, os_core handles the port-level incantations now — TorqueJax
        fn write_byte(b: u8) {
            const SPIN_LIMIT: u32 = 2048;
            unsafe {
                let mut spins: u32 = 0;
                loop {
                    let status = os_core::inb(0x3FD);
                    if status & 0x20 != 0 {
                        break;
                    }
                    spins += 1;
                    if spins >= SPIN_LIMIT {
                        return;
                    }
                }
                os_core::outb(0x3F8, b);
            }
        }
        fn write_str(s: &[u8]) {
            for &b in s {
                write_byte(b);
            }
        }
        fn write_hex(mut n: u32) {
            write_str(b"0x");
            if n == 0 {
                write_byte(b'0');
                return;
            }
            let mut buf = [0u8; 8];
            let mut i = 0;
            while n > 0 {
                let d = (n & 0xF) as u8;
                buf[i] = if d < 10 { b'0' + d } else { b'a' + d - 10 };
                n >>= 4;
                i += 1;
            }
            while i > 0 {
                i -= 1;
                write_byte(buf[i]);
            }
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

/// WireSaint: Decrement mount's open file counter when File is dropped
/// — ColdCipher: Also release any advisory flock held by this file description.
/// This is the "last close releases the lock" guarantee from POSIX.
impl Drop for File {
    fn drop(&mut self) {
        // — ColdCipher: Only touch the flock registry if this file actually
        // holds a lock. Avoids stat() + spin::Mutex on every close.
        // SMP with 4 CPUs closing fds during exec/fork can't afford that.
        if *self.has_flock.get_mut() {
            if let Ok(st) = self.vnode.stat() {
                let inode_id = InodeId { dev: st.dev, ino: st.ino };
                flock::FLOCK_REGISTRY.unlock(inode_id, self.owner_id);
            }
        }

        if let Some(ref counter) = self.mount_ref_count {
            counter.fetch_sub(1, Ordering::Relaxed);
        }
    }
}
