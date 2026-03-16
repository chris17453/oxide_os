//! Pipe implementation
//!
//! Provides anonymous pipe support for inter-process communication.
//!
//! ## 🔥 NOW WITH REAL WAIT QUEUES (Not Vec<u32> Hacks) 🔥
//!
//! — ByteRiot: Previous version stored PIDs in Vec<u32> — heap allocation
//! inside a spinlock, O(N) contains() checks, and take()/retain() gymnastics.
//! Every pipe read/write was a heap alloc+free pair fighting the global spinlock.
//!
//! Now uses fixed-capacity WaitQueues: zero heap allocation, lock-free
//! register/unregister, ISR-safe wake. The pipe buffer Mutex only protects
//! the ring buffer data, not the waiter lists.

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};
use spin::Mutex;
use waitqueue::WaitQueue;

use crate::error::{VfsError, VfsResult};
use crate::vnode::{DirEntry, Mode, Stat, VnodeOps, VnodeType};

// External scheduler functions (linked from kernel)
// These are provided by the sched crate but we can't depend on it directly
// due to circular dependencies (vfs → sched → proc → vfs)
unsafe extern "Rust" {
    /// Block the current task in TASK_INTERRUPTIBLE state
    fn sched_block_interruptible();

    /// Get the current task's PID
    fn sched_current_pid() -> Option<u32>;
}

/// Pipe buffer size (64KB)
const PIPE_BUF_SIZE: usize = 65536;

/// Shared pipe buffer — ring buffer + reader/writer counts.
/// — ByteRiot: Wait queues live OUTSIDE the Mutex now. No more heap
/// allocation under the pipe lock. The Mutex only protects the data ring.
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

/// Shared state for both ends of a pipe.
/// — ByteRiot: WaitQueues live here, outside the buffer Mutex. Register and
/// wake are lock-free atomic operations — no heap, no spinlock contention.
struct PipeShared {
    buffer: Mutex<PipeBuffer>,
    /// — ByteRiot: Readers waiting for data. Woken by write() and drop(PipeWrite).
    read_wq: WaitQueue,
    /// — ByteRiot: Writers waiting for space. Woken by read() and drop(PipeRead).
    write_wq: WaitQueue,
}

/// Read end of a pipe
pub struct PipeRead {
    shared: Arc<PipeShared>,
}

impl PipeRead {
    fn new(shared: Arc<PipeShared>) -> Self {
        PipeRead { shared }
    }
}

impl Drop for PipeRead {
    fn drop(&mut self) {
        self.shared.buffer.lock().readers.fetch_sub(1, Ordering::Release);
        // — ByteRiot: Wake waiting writers — pipe is now broken (no readers).
        // They'll get EPIPE when they retry. Zero allocation, ISR-safe.
        self.shared.write_wq.wake_all();
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
        // — ByteRiot: Block until data arrives or pipe breaks. Now uses
        // WaitQueue instead of Vec<u32> — the read_wq.register() is lock-free,
        // no heap allocation, and wake_all() is ISR-safe.
        loop {
            let (n, has_writers, was_full) = {
                let mut buffer = self.shared.buffer.lock();

                // EOF: buffer empty AND no writers
                if buffer.count == 0 && !buffer.has_writers() {
                    return Ok(0);
                }

                let was_full = buffer.count == PIPE_BUF_SIZE;
                let n = buffer.read(buf);
                let has_writers = buffer.has_writers();

                (n, has_writers, was_full)
            }; // — ByteRiot: Lock released. Wake outside the critical section.

            // Wake writers if we freed up space (buffer was full before read)
            if n > 0 && was_full {
                self.shared.write_wq.wake_all();
            }

            if n > 0 {
                return Ok(n);
            }

            // Buffer empty, writers exist → block and wait
            if has_writers {
                let pid = unsafe { sched_current_pid() };
                if let Some(pid) = pid {
                    if let Some(slot) = self.shared.read_wq.register(pid) {
                        // — ByteRiot: Re-check before blocking (lost wake window)
                        {
                            let buffer = self.shared.buffer.lock();
                            if buffer.count > 0 || !buffer.has_writers() {
                                self.shared.read_wq.unregister(slot);
                                continue;
                            }
                        }
                        unsafe { sched_block_interruptible(); }
                        self.shared.read_wq.unregister(slot);
                    }
                }
            } else {
                return Ok(0); // EOF
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
        let buffer = self.shared.buffer.lock();
        buffer.count > 0 || !buffer.has_writers()
    }

    fn poll_write_ready(&self) -> bool {
        false
    }

    fn poll_register_wait(&self, table: &mut waitqueue::PollTable) {
        // — SableWire: Register on the read wait queue. When data arrives
        // (writer calls wake_all on read_wq), we'll be woken up.
        table.register(&self.shared.read_wq);
    }
}

/// Write end of a pipe
pub struct PipeWrite {
    shared: Arc<PipeShared>,
}

impl PipeWrite {
    fn new(shared: Arc<PipeShared>) -> Self {
        PipeWrite { shared }
    }
}

impl Drop for PipeWrite {
    fn drop(&mut self) {
        self.shared.buffer.lock().writers.fetch_sub(1, Ordering::Release);
        // — ByteRiot: Wake waiting readers — they'll get EOF (no writers, buffer empty).
        self.shared.read_wq.wake_all();
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
        // — ByteRiot: Block until space available or pipe breaks. WaitQueue
        // register/wake is lock-free, zero heap alloc.
        loop {
            let (n, has_readers) = {
                let mut buffer = self.shared.buffer.lock();

                if !buffer.has_readers() {
                    return Err(VfsError::BrokenPipe);
                }

                let n = buffer.write(buf);
                let has_readers = buffer.has_readers();

                (n, has_readers)
            }; // — ByteRiot: Lock released.

            // Wake readers if we added data
            if n > 0 {
                self.shared.read_wq.wake_all();
                return Ok(n);
            }

            // Buffer full, readers exist → block and wait for space
            if has_readers {
                let pid = unsafe { sched_current_pid() };
                if let Some(pid) = pid {
                    if let Some(slot) = self.shared.write_wq.register(pid) {
                        // — ByteRiot: Re-check before blocking (lost wake window)
                        {
                            let buffer = self.shared.buffer.lock();
                            if buffer.count < PIPE_BUF_SIZE || !buffer.has_readers() {
                                self.shared.write_wq.unregister(slot);
                                continue;
                            }
                        }
                        unsafe { sched_block_interruptible(); }
                        self.shared.write_wq.unregister(slot);
                    }
                }
            } else {
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
        false
    }

    fn poll_write_ready(&self) -> bool {
        let buffer = self.shared.buffer.lock();
        buffer.count < PIPE_BUF_SIZE && buffer.has_readers()
    }

    fn poll_register_wait(&self, table: &mut waitqueue::PollTable) {
        // — SableWire: Register on the write wait queue. When a reader drains
        // data (and the buffer was full), we'll be woken up.
        table.register(&self.shared.write_wq);
    }
}

/// Create a new pipe
///
/// Returns (read_end, write_end) as Arc<dyn VnodeOps>
pub fn create_pipe() -> VfsResult<(Arc<dyn VnodeOps>, Arc<dyn VnodeOps>)> {
    let shared = Arc::new(PipeShared {
        buffer: Mutex::new(PipeBuffer::new()),
        read_wq: WaitQueue::new(),
        write_wq: WaitQueue::new(),
    });

    let read_end = Arc::new(PipeRead::new(Arc::clone(&shared)));
    let write_end = Arc::new(PipeWrite::new(shared));

    Ok((read_end, write_end))
}
