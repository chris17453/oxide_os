//! Indexing queue

use alloc::string::String;
use alloc::collections::VecDeque;
use spin::Mutex;

/// Queue item
#[derive(Debug, Clone)]
pub struct QueueItem {
    /// File path
    pub path: String,
    /// Priority (higher = more urgent)
    pub priority: u8,
    /// Retry count
    pub retries: u8,
}

impl QueueItem {
    /// Create new queue item
    pub fn new(path: String, priority: u8) -> Self {
        QueueItem {
            path,
            priority,
            retries: 0,
        }
    }
}

/// Indexing queue
pub struct IndexQueue {
    /// High priority queue
    high: Mutex<VecDeque<QueueItem>>,
    /// Normal priority queue
    normal: Mutex<VecDeque<QueueItem>>,
    /// Low priority queue (background reindex)
    low: Mutex<VecDeque<QueueItem>>,
}

impl IndexQueue {
    /// Create new queue
    pub fn new() -> Self {
        IndexQueue {
            high: Mutex::new(VecDeque::new()),
            normal: Mutex::new(VecDeque::new()),
            low: Mutex::new(VecDeque::new()),
        }
    }

    /// Add item to queue
    pub fn push(&self, item: QueueItem) {
        match item.priority {
            0..=2 => self.low.lock().push_back(item),
            3..=6 => self.normal.lock().push_back(item),
            _ => self.high.lock().push_back(item),
        }
    }

    /// Get next item (priority order)
    pub fn pop(&self) -> Option<QueueItem> {
        // Try high priority first
        if let Some(item) = self.high.lock().pop_front() {
            return Some(item);
        }
        // Then normal
        if let Some(item) = self.normal.lock().pop_front() {
            return Some(item);
        }
        // Finally low
        self.low.lock().pop_front()
    }

    /// Get queue length
    pub fn len(&self) -> usize {
        self.high.lock().len() + self.normal.lock().len() + self.low.lock().len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clear all queues
    pub fn clear(&self) {
        self.high.lock().clear();
        self.normal.lock().clear();
        self.low.lock().clear();
    }
}

impl Default for IndexQueue {
    fn default() -> Self {
        Self::new()
    }
}
