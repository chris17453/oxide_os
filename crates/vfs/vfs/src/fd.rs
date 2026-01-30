//! File descriptor table
//!
//! Each process has a file descriptor table mapping integers to open files.

use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::Mutex;

use crate::error::{VfsError, VfsResult};
use crate::file::File;

/// File descriptor number
pub type Fd = i32;

/// A file descriptor entry
#[derive(Clone)]
pub struct FileDescriptor {
    /// The open file
    pub file: Arc<File>,
    /// Close-on-exec flag
    pub cloexec: bool,
}

impl FileDescriptor {
    pub fn new(file: Arc<File>) -> Self {
        FileDescriptor {
            file,
            cloexec: false,
        }
    }

    pub fn with_cloexec(file: Arc<File>, cloexec: bool) -> Self {
        FileDescriptor { file, cloexec }
    }
}

// DEBUG: Global variables to track alloc behavior
static LAST_ALLOC_ENTRIES_LEN: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
static LAST_ALLOC_RESULT: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(-1);

/// File descriptor table
pub struct FdTable {
    /// The file descriptors (index = fd number)
    entries: Vec<Option<FileDescriptor>>,
    /// Maximum number of open files
    max_fds: usize,
}

impl FdTable {
    /// Default maximum file descriptors
    pub const DEFAULT_MAX_FDS: usize = 256;

    /// Create a new empty file descriptor table
    pub fn new() -> Self {
        FdTable {
            entries: Vec::new(),
            max_fds: Self::DEFAULT_MAX_FDS,
        }
    }

    /// Create with custom maximum
    pub fn with_max(max_fds: usize) -> Self {
        FdTable {
            entries: Vec::new(),
            max_fds,
        }
    }

    /// Allocate the lowest available file descriptor
    pub fn alloc(&mut self, file: Arc<File>) -> VfsResult<Fd> {
        self.alloc_at_least(0, file)
    }

    /// Allocate file descriptor >= min_fd
    pub fn alloc_at_least(&mut self, min_fd: Fd, file: Arc<File>) -> VfsResult<Fd> {
        let min_fd = min_fd.max(0) as usize;

        // DEBUG: Store diagnostic info in global variables
        LAST_ALLOC_ENTRIES_LEN.store(self.entries.len() as u32, core::sync::atomic::Ordering::SeqCst);

        // Find first free slot >= min_fd
        // Original iterator-based approach
        for (i, entry) in self.entries.iter().enumerate() {
            if i >= min_fd && entry.is_none() {
                self.entries[i] = Some(FileDescriptor::new(file));
                LAST_ALLOC_RESULT.store(i as i32, core::sync::atomic::Ordering::SeqCst);
                return Ok(i as Fd);
            }
        }

        // No free slot found, try to extend
        let new_fd = self.entries.len().max(min_fd);
        if new_fd >= self.max_fds {
            return Err(VfsError::TooManyOpenFiles);
        }

        // Extend to new_fd + 1
        self.entries.resize(new_fd + 1, None);
        self.entries[new_fd] = Some(FileDescriptor::new(file));
        LAST_ALLOC_RESULT.store(new_fd as i32, core::sync::atomic::Ordering::SeqCst);
        Ok(new_fd as Fd)
    }

    /// Insert at specific fd (for dup2)
    pub fn insert(&mut self, fd: Fd, file: Arc<File>) -> VfsResult<()> {
        if fd < 0 {
            return Err(VfsError::BadFd);
        }
        let fd = fd as usize;
        if fd >= self.max_fds {
            return Err(VfsError::BadFd);
        }

        // Extend if needed
        if fd >= self.entries.len() {
            self.entries.resize(fd + 1, None);
        }

        self.entries[fd] = Some(FileDescriptor::new(file));
        Ok(())
    }

    /// Get file descriptor
    pub fn get(&self, fd: Fd) -> VfsResult<&FileDescriptor> {
        if fd < 0 {
            return Err(VfsError::BadFd);
        }
        let fd = fd as usize;
        self.entries
            .get(fd)
            .and_then(|e| e.as_ref())
            .ok_or(VfsError::BadFd)
    }

    /// Get mutable file descriptor
    pub fn get_mut(&mut self, fd: Fd) -> VfsResult<&mut FileDescriptor> {
        if fd < 0 {
            return Err(VfsError::BadFd);
        }
        let fd = fd as usize;
        self.entries
            .get_mut(fd)
            .and_then(|e| e.as_mut())
            .ok_or(VfsError::BadFd)
    }

    /// Close a file descriptor
    pub fn close(&mut self, fd: Fd) -> VfsResult<()> {
        if fd < 0 {
            return Err(VfsError::BadFd);
        }
        let fd = fd as usize;
        if fd >= self.entries.len() {
            return Err(VfsError::BadFd);
        }
        if self.entries[fd].is_none() {
            return Err(VfsError::BadFd);
        }
        self.entries[fd] = None;
        Ok(())
    }

    /// Duplicate a file descriptor
    pub fn dup(&mut self, old_fd: Fd) -> VfsResult<Fd> {
        let file = self.get(old_fd)?.file.clone();
        self.alloc(file)
    }

    /// Duplicate to specific fd
    pub fn dup2(&mut self, old_fd: Fd, new_fd: Fd) -> VfsResult<Fd> {
        if old_fd == new_fd {
            // Just verify old_fd is valid
            let _ = self.get(old_fd)?;
            return Ok(new_fd);
        }

        let file = self.get(old_fd)?.file.clone();

        // Close new_fd if open (ignore error if not open)
        let _ = self.close(new_fd);

        self.insert(new_fd, file)?;
        Ok(new_fd)
    }

    /// Clone the fd table (for fork)
    pub fn clone_for_fork(&self) -> Self {
        FdTable {
            entries: self.entries.clone(),
            max_fds: self.max_fds,
        }
    }

    /// Close all cloexec file descriptors (for exec)
    pub fn close_cloexec(&mut self) {
        for entry in self.entries.iter_mut() {
            if let Some(fd) = entry {
                if fd.cloexec {
                    *entry = None;
                }
            }
        }
    }
}

impl FdTable {
    /// Get the number of entries in the table (for debugging)
    pub fn entries_len(&self) -> usize {
        self.entries.len()
    }

    /// Check entries as a bitmask (for debugging - up to 8 entries)
    pub fn entries_filled_mask(&self) -> u8 {
        let mut mask = 0u8;
        for (i, entry) in self.entries.iter().enumerate() {
            if i >= 8 {
                break;
            }
            if entry.is_some() {
                mask |= 1 << i;
            }
        }
        mask
    }

    /// Get last alloc_at_least entries length (for debugging)
    pub fn last_alloc_entries_len() -> u32 {
        LAST_ALLOC_ENTRIES_LEN.load(core::sync::atomic::Ordering::SeqCst)
    }

    /// Get last alloc_at_least result (for debugging)
    pub fn last_alloc_result() -> i32 {
        LAST_ALLOC_RESULT.load(core::sync::atomic::Ordering::SeqCst)
    }
}

impl Default for FdTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Global file descriptor table wrapper for single-process testing
pub struct GlobalFdTable {
    inner: Mutex<FdTable>,
}

impl GlobalFdTable {
    pub const fn new() -> Self {
        GlobalFdTable {
            inner: Mutex::new(FdTable {
                entries: Vec::new(),
                max_fds: FdTable::DEFAULT_MAX_FDS,
            }),
        }
    }

    pub fn alloc(&self, file: Arc<File>) -> VfsResult<Fd> {
        self.inner.lock().alloc(file)
    }

    pub fn get(&self, fd: Fd) -> VfsResult<Arc<File>> {
        Ok(self.inner.lock().get(fd)?.file.clone())
    }

    pub fn close(&self, fd: Fd) -> VfsResult<()> {
        self.inner.lock().close(fd)
    }

    pub fn dup(&self, old_fd: Fd) -> VfsResult<Fd> {
        self.inner.lock().dup(old_fd)
    }

    pub fn dup2(&self, old_fd: Fd, new_fd: Fd) -> VfsResult<Fd> {
        self.inner.lock().dup2(old_fd, new_fd)
    }
}
