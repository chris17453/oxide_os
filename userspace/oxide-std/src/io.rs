//! I/O traits and types compatible with std::io

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

// ============================================================================
// Error Types
// ============================================================================

/// I/O error kind
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    NotFound,
    PermissionDenied,
    ConnectionRefused,
    ConnectionReset,
    ConnectionAborted,
    NotConnected,
    AddrInUse,
    AddrNotAvailable,
    BrokenPipe,
    AlreadyExists,
    WouldBlock,
    InvalidInput,
    InvalidData,
    TimedOut,
    WriteZero,
    Interrupted,
    UnexpectedEof,
    Other,
}

/// I/O error type
#[derive(Debug)]
pub struct Error {
    kind: ErrorKind,
    message: Option<String>,
}

impl Error {
    /// Create a new error with the given kind
    pub fn new(kind: ErrorKind, message: &str) -> Self {
        Error {
            kind,
            message: Some(String::from(message)),
        }
    }

    /// Create an error from a raw OS error code
    pub fn from_raw_os_error(code: i32) -> Self {
        let kind = match code {
            -2 => ErrorKind::NotFound,        // ENOENT
            -13 => ErrorKind::PermissionDenied, // EACCES
            -17 => ErrorKind::AlreadyExists,  // EEXIST
            -9 => ErrorKind::InvalidInput,    // EBADF
            _ => ErrorKind::Other,
        };
        Error { kind, message: None }
    }

    /// Get the error kind
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref msg) = self.message {
            write!(f, "{}", msg)
        } else {
            write!(f, "{:?}", self.kind)
        }
    }
}

/// Result type for I/O operations
pub type Result<T> = core::result::Result<T, Error>;

// ============================================================================
// Read Trait
// ============================================================================

/// Read trait - similar to std::io::Read
pub trait Read {
    /// Read bytes into buffer, returns number of bytes read
    fn read(&mut self, buf: &mut [u8]) -> Result<usize>;

    /// Read exact number of bytes
    fn read_exact(&mut self, buf: &mut [u8]) -> Result<()> {
        let mut total = 0;
        while total < buf.len() {
            match self.read(&mut buf[total..]) {
                Ok(0) => return Err(Error::new(ErrorKind::UnexpectedEof, "unexpected end of file")),
                Ok(n) => total += n,
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    /// Read all bytes until EOF
    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> Result<usize> {
        let mut total = 0;
        let mut tmp = [0u8; 1024];
        loop {
            match self.read(&mut tmp) {
                Ok(0) => return Ok(total),
                Ok(n) => {
                    buf.extend_from_slice(&tmp[..n]);
                    total += n;
                }
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
    }

    /// Read all bytes into a string
    fn read_to_string(&mut self, buf: &mut String) -> Result<usize> {
        let mut bytes = Vec::new();
        let len = self.read_to_end(&mut bytes)?;
        match core::str::from_utf8(&bytes) {
            Ok(s) => {
                buf.push_str(s);
                Ok(len)
            }
            Err(_) => Err(Error::new(ErrorKind::InvalidData, "invalid UTF-8")),
        }
    }

    /// Create a buffered reader
    fn bytes(self) -> Bytes<Self> where Self: Sized {
        Bytes { inner: self }
    }
}

/// Iterator over bytes
pub struct Bytes<R> {
    inner: R,
}

impl<R: Read> Iterator for Bytes<R> {
    type Item = Result<u8>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut buf = [0u8; 1];
        match self.inner.read(&mut buf) {
            Ok(0) => None,
            Ok(_) => Some(Ok(buf[0])),
            Err(e) => Some(Err(e)),
        }
    }
}

// ============================================================================
// Write Trait
// ============================================================================

/// Write trait - similar to std::io::Write
pub trait Write {
    /// Write bytes from buffer, returns number of bytes written
    fn write(&mut self, buf: &[u8]) -> Result<usize>;

    /// Flush output
    fn flush(&mut self) -> Result<()>;

    /// Write all bytes
    fn write_all(&mut self, buf: &[u8]) -> Result<()> {
        let mut total = 0;
        while total < buf.len() {
            match self.write(&buf[total..]) {
                Ok(0) => return Err(Error::new(ErrorKind::WriteZero, "write returned 0")),
                Ok(n) => total += n,
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    /// Write a formatted string
    fn write_fmt(&mut self, fmt: fmt::Arguments<'_>) -> Result<()> {
        // Use a small buffer for formatting
        struct FmtWriter<'a, W: Write + ?Sized> {
            writer: &'a mut W,
            error: Option<Error>,
        }

        impl<W: Write + ?Sized> fmt::Write for FmtWriter<'_, W> {
            fn write_str(&mut self, s: &str) -> fmt::Result {
                match self.writer.write_all(s.as_bytes()) {
                    Ok(()) => Ok(()),
                    Err(e) => {
                        self.error = Some(e);
                        Err(fmt::Error)
                    }
                }
            }
        }

        let mut writer = FmtWriter { writer: self, error: None };
        match fmt::write(&mut writer, fmt) {
            Ok(()) => Ok(()),
            Err(_) => Err(writer.error.unwrap_or_else(|| Error::new(ErrorKind::Other, "format error"))),
        }
    }
}


// ============================================================================
// BufRead Trait
// ============================================================================

/// Buffered read trait
pub trait BufRead: Read {
    /// Read until delimiter or EOF
    fn read_until(&mut self, byte: u8, buf: &mut Vec<u8>) -> Result<usize> {
        let mut total = 0;
        loop {
            let mut b = [0u8; 1];
            match self.read(&mut b) {
                Ok(0) => return Ok(total),
                Ok(_) => {
                    buf.push(b[0]);
                    total += 1;
                    if b[0] == byte {
                        return Ok(total);
                    }
                }
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
    }

    /// Read a line
    fn read_line(&mut self, buf: &mut String) -> Result<usize> {
        let mut bytes = Vec::new();
        let len = self.read_until(b'\n', &mut bytes)?;
        match core::str::from_utf8(&bytes) {
            Ok(s) => {
                buf.push_str(s);
                Ok(len)
            }
            Err(_) => Err(Error::new(ErrorKind::InvalidData, "invalid UTF-8")),
        }
    }

    /// Get lines iterator
    fn lines(self) -> Lines<Self> where Self: Sized {
        Lines { inner: self }
    }
}

/// Iterator over lines
pub struct Lines<B> {
    inner: B,
}

impl<B: BufRead> Iterator for Lines<B> {
    type Item = Result<String>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut line = String::new();
        match self.inner.read_line(&mut line) {
            Ok(0) => None,
            Ok(_) => {
                // Remove trailing newline
                if line.ends_with('\n') {
                    line.pop();
                    if line.ends_with('\r') {
                        line.pop();
                    }
                }
                Some(Ok(line))
            }
            Err(e) => Some(Err(e)),
        }
    }
}

// ============================================================================
// Seek Trait
// ============================================================================

/// Seek position
pub enum SeekFrom {
    Start(u64),
    End(i64),
    Current(i64),
}

/// Seek trait
pub trait Seek {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64>;
}

// ============================================================================
// Standard Streams
// ============================================================================

/// Standard input
pub struct Stdin {
    _private: (),
}

impl Stdin {
    /// Lock stdin (no-op in single-threaded context)
    pub fn lock(&self) -> StdinLock<'_> {
        StdinLock { _marker: core::marker::PhantomData }
    }

    /// Read a line from stdin
    pub fn read_line(&self, buf: &mut String) -> Result<usize> {
        let mut lock = self.lock();
        lock.read_line(buf)
    }
}

impl Read for Stdin {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let n = libc::read(libc::STDIN_FILENO, buf);
        if n < 0 {
            Err(Error::from_raw_os_error(n as i32))
        } else {
            Ok(n as usize)
        }
    }
}

/// Locked stdin
pub struct StdinLock<'a> {
    _marker: core::marker::PhantomData<&'a ()>,
}

impl Read for StdinLock<'_> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let n = libc::read(libc::STDIN_FILENO, buf);
        if n < 0 {
            Err(Error::from_raw_os_error(n as i32))
        } else {
            Ok(n as usize)
        }
    }
}

impl BufRead for StdinLock<'_> {}

/// Get stdin handle
pub fn stdin() -> Stdin {
    Stdin { _private: () }
}

/// Standard output
pub struct Stdout {
    _private: (),
}

impl Stdout {
    /// Lock stdout (no-op in single-threaded context)
    pub fn lock(&self) -> StdoutLock<'_> {
        StdoutLock { _marker: core::marker::PhantomData }
    }
}

impl Write for Stdout {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        let n = libc::write(libc::STDOUT_FILENO, buf);
        if n < 0 {
            Err(Error::from_raw_os_error(n as i32))
        } else {
            Ok(n as usize)
        }
    }

    fn flush(&mut self) -> Result<()> {
        Ok(()) // No buffering
    }
}

/// Locked stdout
pub struct StdoutLock<'a> {
    _marker: core::marker::PhantomData<&'a ()>,
}

impl Write for StdoutLock<'_> {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        let n = libc::write(libc::STDOUT_FILENO, buf);
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

/// Get stdout handle
pub fn stdout() -> Stdout {
    Stdout { _private: () }
}

/// Standard error
pub struct Stderr {
    _private: (),
}

impl Stderr {
    /// Lock stderr
    pub fn lock(&self) -> StderrLock<'_> {
        StderrLock { _marker: core::marker::PhantomData }
    }
}

impl Write for Stderr {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        // Write to stderr (fd 2), but OXIDE may not have separate stderr
        // so we write to stdout for now
        let n = libc::write(libc::STDOUT_FILENO, buf);
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

/// Locked stderr
pub struct StderrLock<'a> {
    _marker: core::marker::PhantomData<&'a ()>,
}

impl Write for StderrLock<'_> {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        let n = libc::write(libc::STDOUT_FILENO, buf);
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

/// Get stderr handle
pub fn stderr() -> Stderr {
    Stderr { _private: () }
}

// ============================================================================
// BufReader and BufWriter
// ============================================================================

/// Buffered reader
pub struct BufReader<R> {
    inner: R,
    buf: Vec<u8>,
    pos: usize,
    cap: usize,
}

impl<R: Read> BufReader<R> {
    /// Create a new buffered reader
    pub fn new(inner: R) -> Self {
        Self::with_capacity(8192, inner)
    }

    /// Create with specific capacity
    pub fn with_capacity(capacity: usize, inner: R) -> Self {
        let mut buf = Vec::with_capacity(capacity);
        buf.resize(capacity, 0);
        BufReader {
            inner,
            buf,
            pos: 0,
            cap: 0,
        }
    }

    /// Get inner reader
    pub fn into_inner(self) -> R {
        self.inner
    }

    fn fill_buf(&mut self) -> Result<&[u8]> {
        if self.pos >= self.cap {
            self.cap = self.inner.read(&mut self.buf)?;
            self.pos = 0;
        }
        Ok(&self.buf[self.pos..self.cap])
    }

    fn consume(&mut self, amt: usize) {
        self.pos = core::cmp::min(self.pos + amt, self.cap);
    }
}

impl<R: Read> Read for BufReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let available = self.fill_buf()?;
        let amt = core::cmp::min(available.len(), buf.len());
        buf[..amt].copy_from_slice(&available[..amt]);
        self.consume(amt);
        Ok(amt)
    }
}

impl<R: Read> BufRead for BufReader<R> {}

/// Buffered writer
pub struct BufWriter<W: Write> {
    inner: Option<W>,
    buf: Vec<u8>,
}

impl<W: Write> BufWriter<W> {
    /// Create a new buffered writer
    pub fn new(inner: W) -> Self {
        Self::with_capacity(8192, inner)
    }

    /// Create with specific capacity
    pub fn with_capacity(capacity: usize, inner: W) -> Self {
        BufWriter {
            inner: Some(inner),
            buf: Vec::with_capacity(capacity),
        }
    }

    /// Get inner writer
    pub fn into_inner(mut self) -> core::result::Result<W, Error> {
        self.flush_internal()?;
        // Take the inner writer, leaving None (drop won't try to flush)
        self.inner.take().ok_or_else(|| Error::new(ErrorKind::Other, "already consumed"))
    }

    fn flush_internal(&mut self) -> Result<()> {
        if let Some(ref mut inner) = self.inner {
            inner.write_all(&self.buf)?;
            self.buf.clear();
            inner.flush()
        } else {
            Ok(())
        }
    }
}

impl<W: Write> Write for BufWriter<W> {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        if self.buf.len() + buf.len() > self.buf.capacity() {
            self.flush()?;
        }
        if let Some(ref mut inner) = self.inner {
            if buf.len() >= self.buf.capacity() {
                inner.write(buf)
            } else {
                self.buf.extend_from_slice(buf);
                Ok(buf.len())
            }
        } else {
            Err(Error::new(ErrorKind::Other, "writer consumed"))
        }
    }

    fn flush(&mut self) -> Result<()> {
        self.flush_internal()
    }
}

impl<W: Write> Drop for BufWriter<W> {
    fn drop(&mut self) {
        let _ = self.flush_internal();
    }
}
