//! NVMe Queue Management

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

use crate::{NvmeCqe, NvmeSqe};

/// Submission queue
pub struct SubmissionQueue {
    /// Queue entries
    entries: Mutex<Vec<NvmeSqe>>,
    /// Queue size (number of entries)
    size: u16,
    /// Tail pointer
    tail: AtomicU32,
    /// Doorbell address
    doorbell: u64,
}

impl SubmissionQueue {
    /// Create a new submission queue
    pub fn new(size: u16, doorbell: u64) -> Self {
        let mut entries = Vec::with_capacity(size as usize);
        entries.resize(size as usize, NvmeSqe::default());

        SubmissionQueue {
            entries: Mutex::new(entries),
            size,
            tail: AtomicU32::new(0),
            doorbell,
        }
    }

    /// Submit a command to the queue
    ///
    /// Returns the slot index used.
    pub fn submit(&self, cmd: NvmeSqe) -> u32 {
        let tail = self.tail.load(Ordering::Acquire);
        let slot = tail % self.size as u32;

        {
            let mut entries = self.entries.lock();
            entries[slot as usize] = cmd;
        }

        let new_tail = (tail + 1) % self.size as u32;
        self.tail.store(new_tail, Ordering::Release);

        // Ring doorbell
        unsafe {
            core::ptr::write_volatile(self.doorbell as *mut u32, new_tail);
        }

        slot
    }

    /// Get current tail
    pub fn tail(&self) -> u32 {
        self.tail.load(Ordering::Acquire)
    }
}

/// Completion queue
pub struct CompletionQueue {
    /// Queue entries
    entries: Mutex<Vec<NvmeCqe>>,
    /// Queue size
    size: u16,
    /// Head pointer
    head: AtomicU32,
    /// Current phase bit
    phase: AtomicU32,
    /// Doorbell address
    doorbell: u64,
}

impl CompletionQueue {
    /// Create a new completion queue
    pub fn new(size: u16, doorbell: u64) -> Self {
        let mut entries = Vec::with_capacity(size as usize);
        entries.resize(size as usize, NvmeCqe::default());

        CompletionQueue {
            entries: Mutex::new(entries),
            size,
            head: AtomicU32::new(0),
            phase: AtomicU32::new(1),
            doorbell,
        }
    }

    /// Poll for a completion
    ///
    /// Returns the completion entry if one is available.
    pub fn poll(&self) -> Option<NvmeCqe> {
        let head = self.head.load(Ordering::Acquire);
        let phase = self.phase.load(Ordering::Acquire) as u16;
        let slot = head % self.size as u32;

        let cqe = {
            let entries = self.entries.lock();
            entries[slot as usize]
        };

        // Check phase bit
        if (cqe.status & 1) != phase {
            return None;
        }

        // Advance head
        let new_head = (head + 1) % self.size as u32;
        self.head.store(new_head, Ordering::Release);

        // Check for phase wrap
        if new_head == 0 {
            let old_phase = self.phase.load(Ordering::Acquire);
            self.phase.store(1 - old_phase, Ordering::Release);
        }

        // Ring doorbell
        unsafe {
            core::ptr::write_volatile(self.doorbell as *mut u32, new_head);
        }

        Some(cqe)
    }

    /// Get current head
    pub fn head(&self) -> u32 {
        self.head.load(Ordering::Acquire)
    }

    /// Get current phase
    pub fn phase(&self) -> u32 {
        self.phase.load(Ordering::Acquire)
    }
}

/// Queue pair (SQ + CQ)
pub struct QueuePair {
    /// Queue ID
    pub qid: u16,
    /// Submission queue
    pub sq: SubmissionQueue,
    /// Completion queue
    pub cq: CompletionQueue,
}

impl QueuePair {
    /// Create a new queue pair
    pub fn new(qid: u16, size: u16, sq_doorbell: u64, cq_doorbell: u64) -> Self {
        QueuePair {
            qid,
            sq: SubmissionQueue::new(size, sq_doorbell),
            cq: CompletionQueue::new(size, cq_doorbell),
        }
    }
}
