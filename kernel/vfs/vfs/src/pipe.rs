//! Pipe implementation
//!
//! Provides anonymous pipe support for inter-process communication.
//!
//! ## 🔥 NOW WITH ACTUAL BLOCKING (Not Just EAGAIN) 🔥
//!
//! Previous version returned `VfsError::WouldBlock` (EAGAIN) and hoped the syscall
//! layer would deal with it. Narrator: *it didn't*.
//!
//! Result: `cat file | grep pattern` got EAGAIN spam instead of blocking.
//!
//! Now uses **proper wait queues** like a real OS:
//! - Read on empty pipe → add to read_queue → `TASK_INTERRUPTIBLE` sleep
//! - Write arrives → wake all readers → they drain the data
//! - Write on full pipe → add to write_queue → sleep
//! - Read happens → wake all writers → they write more data

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};
use spin::Mutex;

use crate::error::{VfsError, VfsResult};
use crate::vnode::{DirEntry, Mode, Stat, VnodeOps, VnodeType};

// External scheduler functions (linked from kernel)
// These are provided by the sched crate but we can't depend on it directly
// due to circular dependencies (vfs → sched → proc → vfs)
unsafe extern "Rust" {
    /// Block the current task in TASK_INTERRUPTIBLE state
    fn sched_block_interruptible();

    /// Wake up a task by PID
    fn sched_wake_up(pid: u32);

    /// Get the current task's PID
    fn sched_current_pid() -> Option<u32>;
}

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
    /// PIDs of processes waiting to read (buffer empty)
    read_waiters: Vec<u32>,
    /// PIDs of processes waiting to write (buffer full)
    write_waiters: Vec<u32>,
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
            read_waiters: Vec::new(),
            write_waiters: Vec::new(),
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
        let write_waiters = {
            let mut buf = self.buffer.lock();
            buf.readers.fetch_sub(1, Ordering::Release);

            // Wake waiting writers - pipe is now broken (no readers)
            // They'll get EPIPE when they retry
            let waiters = buf.write_waiters.clone();
            buf.write_waiters.clear();
            waiters
        };

        for pid in write_waiters {
            unsafe {
                sched_wake_up(pid);
            }
        }
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
        // 🔥 NOW WITH ACTUAL BLOCKING (Not Just EAGAIN) 🔥
        // Loop until we get data or reach EOF
        loop {
            // Try to read with lock held (keep critical section small)
            let (n, has_writers, write_waiters) = {
                let mut buffer = self.buffer.lock();

                // EOF condition: buffer empty AND no writers exist
                if buffer.count == 0 && !buffer.has_writers() {
                    return Ok(0); // EOF
                }

                // Try to read available data
                let n = buffer.read(buf);
                let has_writers = buffer.has_writers();

                // If we read data and buffer was full, collect waiting writers to wake
                let write_waiters = if n > 0 && buffer.count + n == PIPE_BUF_SIZE {
                    let waiters = buffer.write_waiters.clone();
                    buffer.write_waiters.clear();
                    waiters
                } else {
                    Vec::new()
                };

                (n, has_writers, write_waiters)
            }; // Release lock!

            // Wake writers if we freed up space
            for pid in write_waiters {
                unsafe {
                    sched_wake_up(pid);
                }
            }

            // If we got data, return it
            if n > 0 {
                return Ok(n);
            }

            // Buffer empty but writers exist → block and wait for data
            if has_writers {
                // Get current PID and add to wait queue
                let pid = unsafe { sched_current_pid() };
                if let Some(pid) = pid {
                    {
                        let mut buffer = self.buffer.lock();
                        if !buffer.read_waiters.contains(&pid) {
                            buffer.read_waiters.push(pid);
                        }
                    }

                    // Block in interruptible sleep
                    unsafe {
                        sched_block_interruptible();
                    }

                    // When we wake up (by signal or writer), remove ourselves and retry
                    let mut buffer = self.buffer.lock();
                    buffer.read_waiters.retain(|&p| p != pid);
                }
                // Loop back and retry the read
            } else {
                // No writers and no data → EOF
                return Ok(0);
            }
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

    fn poll_read_ready(&self) -> bool {
        let buffer = self.buffer.lock();
        // Read end is ready if there's data or no writers (EOF condition)
        buffer.count > 0 || !buffer.has_writers()
    }

    fn poll_write_ready(&self) -> bool {
        // Read end cannot be written to
        false
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
        let read_waiters = {
            let mut buf = self.buffer.lock();
            buf.writers.fetch_sub(1, Ordering::Release);

            // Wake waiting readers - they'll get EOF (no writers, buffer empty)
            let waiters = buf.read_waiters.clone();
            buf.read_waiters.clear();
            waiters
        };

        for pid in read_waiters {
            unsafe {
                sched_wake_up(pid);
            }
        }
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
        // 🔥 NOW WITH ACTUAL BLOCKING (Not Just EAGAIN) 🔥
        // Loop until we write some data or pipe breaks
        loop {
            // Try to write with lock held
            let (n, has_readers, read_waiters) = {
                let mut buffer = self.buffer.lock();

                // If no readers, return EPIPE (broken pipe)
                if !buffer.has_readers() {
                    return Err(VfsError::BrokenPipe);
                }

                // Try to write available data
                let n = buffer.write(buf);
                let has_readers = buffer.has_readers();

                // If we wrote data, collect waiting readers to wake
                let read_waiters = if n > 0 {
                    let waiters = buffer.read_waiters.clone();
                    buffer.read_waiters.clear();
                    waiters
                } else {
                    Vec::new()
                };

                (n, has_readers, read_waiters)
            }; // Release lock!

            // Wake readers if we added data
            for pid in read_waiters {
                unsafe {
                    sched_wake_up(pid);
                }
            }

            // If we wrote data, return it
            if n > 0 {
                return Ok(n);
            }

            // Buffer full but readers exist → block and wait for space
            if has_readers {
                // Get current PID and add to wait queue
                let pid = unsafe { sched_current_pid() };
                if let Some(pid) = pid {
                    {
                        let mut buffer = self.buffer.lock();
                        if !buffer.write_waiters.contains(&pid) {
                            buffer.write_waiters.push(pid);
                        }
                    }

                    // Block in interruptible sleep
                    unsafe {
                        sched_block_interruptible();
                    }

                    // When we wake up (by signal or reader), remove ourselves and retry
                    let mut buffer = self.buffer.lock();
                    buffer.write_waiters.retain(|&p| p != pid);
                }
                // Loop back and retry the write
            } else {
                // No readers → broken pipe
                return Err(VfsError::BrokenPipe);
            }
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

    fn poll_read_ready(&self) -> bool {
        // Write end cannot be read from
        false
    }

    fn poll_write_ready(&self) -> bool {
        let buffer = self.buffer.lock();
        // Write end is ready if there's buffer space and there are readers
        buffer.count < PIPE_BUF_SIZE && buffer.has_readers()
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
