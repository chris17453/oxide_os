//! Filesystem operations compatible with std::fs
//!
//! Provides basic file operations using OXIDE syscalls.

use crate::io::{self, Error, ErrorKind, Read, Result, Seek, SeekFrom, Write};
use alloc::string::{String, ToString};
use alloc::vec::Vec;

// ============================================================================
// File Type
// ============================================================================

/// A file handle
pub struct File {
    fd: i32,
}

impl File {
    /// Open a file for reading
    pub fn open(path: &str) -> Result<File> {
        Self::open_with_options(path, OpenOptions::new().read(true))
    }

    /// Create a new file for writing (truncates if exists)
    pub fn create(path: &str) -> Result<File> {
        Self::open_with_options(
            path,
            OpenOptions::new().write(true).create(true).truncate(true),
        )
    }

    /// Open with specific options
    fn open_with_options(path: &str, opts: &OpenOptions) -> Result<File> {
        let mut flags = 0u32;

        if opts.read && opts.write {
            flags |= libc::O_RDWR;
        } else if opts.write {
            flags |= libc::O_WRONLY;
        } else {
            flags |= libc::O_RDONLY;
        }

        if opts.create {
            flags |= libc::O_CREAT;
        }
        if opts.append {
            flags |= libc::O_APPEND;
        }
        if opts.truncate {
            flags |= libc::O_TRUNC;
        }

        let fd = libc::open(path, flags, 0o644);
        if fd < 0 {
            Err(Error::from_raw_os_error(fd))
        } else {
            Ok(File { fd })
        }
    }

    /// Get the raw file descriptor
    pub fn as_raw_fd(&self) -> i32 {
        self.fd
    }

    /// Sync all data to disk
    pub fn sync_all(&self) -> Result<()> {
        Ok(())
    }
}

impl Read for File {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let n = libc::read(self.fd, buf);
        if n < 0 {
            Err(Error::from_raw_os_error(n as i32))
        } else {
            Ok(n as usize)
        }
    }
}

impl Write for File {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        let n = libc::write(self.fd, buf);
        if n < 0 {
            Err(Error::from_raw_os_error(n as i32))
        } else {
            Ok(n as usize)
        }
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

impl Seek for File {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64> {
        let (offset, whence) = match pos {
            SeekFrom::Start(n) => (n as i64, libc::SEEK_SET),
            SeekFrom::End(n) => (n, libc::SEEK_END),
            SeekFrom::Current(n) => (n, libc::SEEK_CUR),
        };
        let result = libc::lseek(self.fd, offset, whence);
        if result < 0 {
            Err(Error::from_raw_os_error(result as i32))
        } else {
            Ok(result as u64)
        }
    }
}

impl Drop for File {
    fn drop(&mut self) {
        let _ = libc::close(self.fd);
    }
}

// ============================================================================
// OpenOptions
// ============================================================================

/// Options for opening files
#[derive(Clone, Default)]
pub struct OpenOptions {
    read: bool,
    write: bool,
    append: bool,
    truncate: bool,
    create: bool,
    create_new: bool,
}

impl OpenOptions {
    /// Create new options
    pub fn new() -> Self {
        Self::default()
    }

    /// Set read access
    pub fn read(&mut self, read: bool) -> &mut Self {
        self.read = read;
        self
    }

    /// Set write access
    pub fn write(&mut self, write: bool) -> &mut Self {
        self.write = write;
        self
    }

    /// Set append mode
    pub fn append(&mut self, append: bool) -> &mut Self {
        self.append = append;
        self
    }

    /// Set truncate mode
    pub fn truncate(&mut self, truncate: bool) -> &mut Self {
        self.truncate = truncate;
        self
    }

    /// Set create mode
    pub fn create(&mut self, create: bool) -> &mut Self {
        self.create = create;
        self
    }

    /// Set create_new mode
    pub fn create_new(&mut self, create_new: bool) -> &mut Self {
        self.create_new = create_new;
        self
    }

    /// Open a file with these options
    pub fn open(&self, path: &str) -> Result<File> {
        File::open_with_options(path, self)
    }
}

// ============================================================================
// Free Functions
// ============================================================================

/// Read entire file to string
pub fn read_to_string(path: &str) -> Result<String> {
    let mut file = File::open(path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    Ok(contents)
}

/// Read entire file to bytes
pub fn read(path: &str) -> Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let mut contents = Vec::new();
    file.read_to_end(&mut contents)?;
    Ok(contents)
}

/// Write bytes to file
pub fn write(path: &str, contents: &[u8]) -> Result<()> {
    let mut file = File::create(path)?;
    file.write_all(contents)
}

/// Copy file
pub fn copy(from: &str, to: &str) -> Result<u64> {
    let contents = read(from)?;
    write(to, &contents)?;
    Ok(contents.len() as u64)
}

/// Check if path exists (simple implementation)
pub fn exists(path: &str) -> bool {
    File::open(path).is_ok()
}
