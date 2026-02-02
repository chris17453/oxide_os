//! I/O Scheduler implementations

use alloc::collections::VecDeque;
use alloc::vec::Vec;

use crate::request::Request;

/// I/O Scheduler trait
///
/// Schedulers reorder requests to optimize disk access patterns.
pub trait Scheduler: Send + Sync {
    /// Add a request to the scheduler
    fn add(&mut self, request: Request);

    /// Get the next request to process
    fn next(&mut self) -> Option<Request>;

    /// Check if scheduler has pending requests
    fn has_pending(&self) -> bool;

    /// Get number of pending requests
    fn pending_count(&self) -> usize;
}

/// No-op scheduler - FIFO ordering
///
/// Simply processes requests in the order they arrive.
pub struct NoopScheduler {
    queue: VecDeque<Request>,
}

impl NoopScheduler {
    /// Create a new no-op scheduler
    pub fn new() -> Self {
        NoopScheduler {
            queue: VecDeque::new(),
        }
    }
}

impl Default for NoopScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Scheduler for NoopScheduler {
    fn add(&mut self, request: Request) {
        self.queue.push_back(request);
    }

    fn next(&mut self) -> Option<Request> {
        self.queue.pop_front()
    }

    fn has_pending(&self) -> bool {
        !self.queue.is_empty()
    }

    fn pending_count(&self) -> usize {
        self.queue.len()
    }
}

/// Deadline scheduler
///
/// Batches reads and writes separately to minimize head movement,
/// while ensuring requests don't wait too long.
pub struct DeadlineScheduler {
    /// Read requests
    reads: VecDeque<Request>,
    /// Write requests
    writes: VecDeque<Request>,
    /// Currently servicing reads?
    servicing_reads: bool,
    /// Read batch size
    read_batch: usize,
    /// Write batch size
    write_batch: usize,
    /// Counter for current batch
    batch_count: usize,
}

impl DeadlineScheduler {
    /// Create a new deadline scheduler
    pub fn new() -> Self {
        DeadlineScheduler {
            reads: VecDeque::new(),
            writes: VecDeque::new(),
            servicing_reads: true,
            read_batch: 16,
            write_batch: 8,
            batch_count: 0,
        }
    }

    /// Set batch sizes
    pub fn set_batches(&mut self, read_batch: usize, write_batch: usize) {
        self.read_batch = read_batch;
        self.write_batch = write_batch;
    }
}

impl Default for DeadlineScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Scheduler for DeadlineScheduler {
    fn add(&mut self, request: Request) {
        match request.req_type {
            crate::request::RequestType::Read => {
                self.reads.push_back(request);
            }
            crate::request::RequestType::Write | crate::request::RequestType::Flush => {
                self.writes.push_back(request);
            }
            crate::request::RequestType::Discard => {
                // Discards go with writes
                self.writes.push_back(request);
            }
        }
    }

    fn next(&mut self) -> Option<Request> {
        let batch_limit = if self.servicing_reads {
            self.read_batch
        } else {
            self.write_batch
        };

        // Check if we need to switch batches
        if self.batch_count >= batch_limit {
            self.batch_count = 0;
            self.servicing_reads = !self.servicing_reads;
        }

        // Try to get from current queue
        let result = if self.servicing_reads {
            if let Some(req) = self.reads.pop_front() {
                Some(req)
            } else if let Some(req) = self.writes.pop_front() {
                // Nothing in reads, switch to writes
                self.servicing_reads = false;
                self.batch_count = 0;
                Some(req)
            } else {
                None
            }
        } else {
            if let Some(req) = self.writes.pop_front() {
                Some(req)
            } else if let Some(req) = self.reads.pop_front() {
                // Nothing in writes, switch to reads
                self.servicing_reads = true;
                self.batch_count = 0;
                Some(req)
            } else {
                None
            }
        };

        if result.is_some() {
            self.batch_count += 1;
        }

        result
    }

    fn has_pending(&self) -> bool {
        !self.reads.is_empty() || !self.writes.is_empty()
    }

    fn pending_count(&self) -> usize {
        self.reads.len() + self.writes.len()
    }
}

/// CFQ-like scheduler with sorted queues
///
/// Sorts requests by block number within each batch to minimize seek time.
pub struct CfqScheduler {
    /// Sorted queue
    queue: Vec<Request>,
}

impl CfqScheduler {
    /// Create a new CFQ scheduler
    pub fn new() -> Self {
        CfqScheduler { queue: Vec::new() }
    }
}

impl Default for CfqScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Scheduler for CfqScheduler {
    fn add(&mut self, request: Request) {
        // Insert sorted by start_block
        let pos = self
            .queue
            .iter()
            .position(|r| r.start_block > request.start_block)
            .unwrap_or(self.queue.len());
        self.queue.insert(pos, request);
    }

    fn next(&mut self) -> Option<Request> {
        if self.queue.is_empty() {
            None
        } else {
            Some(self.queue.remove(0))
        }
    }

    fn has_pending(&self) -> bool {
        !self.queue.is_empty()
    }

    fn pending_count(&self) -> usize {
        self.queue.len()
    }
}
