//! I/O Request queue for block devices

use alloc::collections::VecDeque;
use alloc::vec::Vec;
use spin::Mutex;

use crate::BlockResult;

/// Request type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestType {
    /// Read request
    Read,
    /// Write request
    Write,
    /// Flush request
    Flush,
    /// Discard/trim request
    Discard,
}

/// An I/O request
#[derive(Debug)]
pub struct Request {
    /// Request type
    pub req_type: RequestType,
    /// Starting block number
    pub start_block: u64,
    /// Number of blocks
    pub block_count: u64,
    /// Data buffer (for read/write)
    pub buffer: Option<Vec<u8>>,
    /// Request priority (higher = more important)
    pub priority: u8,
    /// Request ID for tracking
    pub id: u64,
}

impl Request {
    /// Create a new read request
    pub fn read(start_block: u64, block_count: u64, buffer_size: usize) -> Self {
        Request {
            req_type: RequestType::Read,
            start_block,
            block_count,
            buffer: Some(alloc::vec![0u8; buffer_size]),
            priority: 0,
            id: 0,
        }
    }

    /// Create a new write request
    pub fn write(start_block: u64, data: Vec<u8>, block_count: u64) -> Self {
        Request {
            req_type: RequestType::Write,
            start_block,
            block_count,
            buffer: Some(data),
            priority: 0,
            id: 0,
        }
    }

    /// Create a flush request
    pub fn flush() -> Self {
        Request {
            req_type: RequestType::Flush,
            start_block: 0,
            block_count: 0,
            buffer: None,
            priority: 255, // High priority
            id: 0,
        }
    }

    /// Create a discard request
    pub fn discard(start_block: u64, block_count: u64) -> Self {
        Request {
            req_type: RequestType::Discard,
            start_block,
            block_count,
            buffer: None,
            priority: 0,
            id: 0,
        }
    }
}

/// Request queue for managing I/O requests
pub struct RequestQueue {
    /// Pending requests
    pending: Mutex<VecDeque<Request>>,
    /// Next request ID
    next_id: core::sync::atomic::AtomicU64,
    /// Maximum queue depth
    max_depth: usize,
}

impl RequestQueue {
    /// Create a new request queue
    pub fn new(max_depth: usize) -> Self {
        RequestQueue {
            pending: Mutex::new(VecDeque::new()),
            next_id: core::sync::atomic::AtomicU64::new(1),
            max_depth,
        }
    }

    /// Submit a request to the queue
    pub fn submit(&self, mut request: Request) -> BlockResult<u64> {
        let mut pending = self.pending.lock();

        if pending.len() >= self.max_depth {
            return Err(crate::BlockError::Busy);
        }

        request.id = self
            .next_id
            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        let id = request.id;
        pending.push_back(request);

        Ok(id)
    }

    /// Get the next request from the queue
    pub fn pop(&self) -> Option<Request> {
        self.pending.lock().pop_front()
    }

    /// Check if queue is empty
    pub fn is_empty(&self) -> bool {
        self.pending.lock().is_empty()
    }

    /// Get number of pending requests
    pub fn len(&self) -> usize {
        self.pending.lock().len()
    }

    /// Get maximum queue depth
    pub fn max_depth(&self) -> usize {
        self.max_depth
    }
}

impl Default for RequestQueue {
    fn default() -> Self {
        Self::new(256)
    }
}
