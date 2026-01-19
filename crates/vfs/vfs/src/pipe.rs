//! Pipe implementation
//!
//! Provides anonymous pipe support for inter-process communication.

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};
use spin::Mutex;

use crate::error::{VfsError, VfsResult};
use crate::vnode::{DirEntry, Mode, Stat, VnodeOps, VnodeType};

/// Pipe buffer size (64KB)
const PIPE_BUF_SIZE: usize = 65536;

/// Shared pipe buffer
struct PipeBuffer {
    /// Ring buffer data
    data: Vec<u8>,
    /// Read position
    read_pos: usize,
    /// Write position
    write_pos: usize,
    /// Current bytes in buffer
    count: usize,
    /// Number of readers
    readers: AtomicUsize,
    /// Number of writers
    writers: AtomicUsize,
}

impl PipeBuffer {
    fn new() -> Self {
        let mut data = Vec::with_capacity(PIPE_BUF_SIZE);
        data.resize(PIPE_BUF_SIZE, 0);
        PipeBuffer {
            data,
            read_pos: 0,
            write_pos: 0,
            count: 0,
            readers: AtomicUsize::new(1),
            writers: AtomicUsize::new(1),
        }
    }

    fn read(&mut self, buf: &mut [u8]) -> usize {
        if self.count == 0 {
            return 0;
        }

        let to_read = buf.len().min(self.count);
        let mut read = 0;

        while read < to_read {
            buf[read] = self.data[self.read_pos];
            self.read_pos = (self.read_pos + 1) % PIPE_BUF_SIZE;
            read += 1;
        }

        self.count -= read;
        read
    }

    fn write(&mut self, buf: &[u8]) -> usize {
        let available = PIPE_BUF_SIZE - self.count;
        if available == 0 {
            return 0;
        }

        let to_write = buf.len().min(available);
        let mut written = 0;

        while written < to_write {
            self.data[self.write_pos] = buf[written];
            self.write_pos = (self.write_pos + 1) % PIPE_BUF_SIZE;
            written += 1;
        }

        self.count += written;
        written
    }

    fn has_writers(&self) -> bool {
        self.writers.load(Ordering::Acquire) > 0
    }

    fn has_readers(&self) -> bool {
        self.readers.load(Ordering::Acquire) > 0
    }
}

/// Read end of a pipe
pub struct PipeRead {
    buffer: Arc<Mutex<PipeBuffer>>,
}

impl PipeRead {
    fn new(buffer: Arc<Mutex<PipeBuffer>>) -> Self {
        PipeRead { buffer }
    }
}

impl Drop for PipeRead {
    fn drop(&mut self) {
        let buf = self.buffer.lock();
        buf.readers.fetch_sub(1, Ordering::Release);
    }
}

impl VnodeOps for PipeRead {
    fn vtype(&self) -> VnodeType {
        VnodeType::Fifo
    }

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotSupported)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotSupported)
    }

    fn read(&self, _offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        let mut buffer = self.buffer.lock();

        // If buffer is empty and no writers, return EOF
        if buffer.count == 0 && !buffer.has_writers() {
            return Ok(0);
        }

        // Read available data
        let n = buffer.read(buf);
        if n > 0 {
            Ok(n)
        } else {
            // Would block - return EAGAIN
            // In a real implementation, we'd block the process
            Err(VfsError::WouldBlock)
        }
    }

    fn write(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::InvalidOperation)
    }

    fn readdir(&self, _offset: u64) -> VfsResult<Option<DirEntry>> {
        Err(VfsError::NotSupported)
    }

    fn mkdir(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotSupported)
    }

    fn rmdir(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }

    fn unlink(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }

    fn rename(&self, _old_name: &str, _new_dir: &dyn VnodeOps, _new_name: &str) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }

    fn stat(&self) -> VfsResult<Stat> {
        Ok(Stat::new(VnodeType::Fifo, Mode::new(0o600), 0, 0))
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }
}

/// Write end of a pipe
pub struct PipeWrite {
    buffer: Arc<Mutex<PipeBuffer>>,
}

impl PipeWrite {
    fn new(buffer: Arc<Mutex<PipeBuffer>>) -> Self {
        PipeWrite { buffer }
    }
}

impl Drop for PipeWrite {
    fn drop(&mut self) {
        let buf = self.buffer.lock();
        buf.writers.fetch_sub(1, Ordering::Release);
    }
}

impl VnodeOps for PipeWrite {
    fn vtype(&self) -> VnodeType {
        VnodeType::Fifo
    }

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotSupported)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotSupported)
    }

    fn read(&self, _offset: u64, _buf: &mut [u8]) -> VfsResult<usize> {
        Err(VfsError::InvalidOperation)
    }

    fn write(&self, _offset: u64, buf: &[u8]) -> VfsResult<usize> {
        let mut buffer = self.buffer.lock();

        // If no readers, return broken pipe
        if !buffer.has_readers() {
            return Err(VfsError::BrokenPipe);
        }

        // Write available data
        let n = buffer.write(buf);
        if n > 0 {
            Ok(n)
        } else {
            // Would block - return EAGAIN
            Err(VfsError::WouldBlock)
        }
    }

    fn readdir(&self, _offset: u64) -> VfsResult<Option<DirEntry>> {
        Err(VfsError::NotSupported)
    }

    fn mkdir(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::NotSupported)
    }

    fn rmdir(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }

    fn unlink(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }

    fn rename(&self, _old_name: &str, _new_dir: &dyn VnodeOps, _new_name: &str) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }

    fn stat(&self) -> VfsResult<Stat> {
        Ok(Stat::new(VnodeType::Fifo, Mode::new(0o600), 0, 0))
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }
}

/// Create a new pipe
///
/// Returns (read_end, write_end) as Arc<dyn VnodeOps>
pub fn create_pipe() -> VfsResult<(Arc<dyn VnodeOps>, Arc<dyn VnodeOps>)> {
    let buffer = Arc::new(Mutex::new(PipeBuffer::new()));

    let read_end = Arc::new(PipeRead::new(Arc::clone(&buffer)));
    let write_end = Arc::new(PipeWrite::new(buffer));

    Ok((read_end, write_end))
}
