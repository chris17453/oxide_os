//! Round-robin scheduler implementation

use alloc::collections::VecDeque;
use alloc::vec::Vec;
use efflux_sched_traits::{Context, Scheduler, Thread, ThreadId, ThreadState};

use crate::KernelThread;

/// Time slice in ticks before preemption
const TIME_SLICE: u32 = 10;

/// Round-robin scheduler
///
/// Simple scheduler that runs each thread for a fixed time slice,
/// then moves to the next thread in the queue.
pub struct RoundRobinScheduler<C: Context> {
    /// All threads in the system
    threads: Vec<KernelThread<C>>,
    /// Queue of ready thread indices
    ready_queue: VecDeque<usize>,
    /// Index of currently running thread (if any)
    current: Option<usize>,
    /// Ticks remaining in current time slice
    ticks_remaining: u32,
    /// Next thread ID to allocate
    next_tid: ThreadId,
}

impl<C: Context> RoundRobinScheduler<C> {
    /// Create a new empty scheduler
    pub const fn new() -> Self {
        Self {
            threads: Vec::new(),
            ready_queue: VecDeque::new(),
            current: None,
            ticks_remaining: TIME_SLICE,
            next_tid: 1,
        }
    }

    /// Allocate a new thread ID
    pub fn alloc_tid(&mut self) -> ThreadId {
        let tid = self.next_tid;
        self.next_tid += 1;
        tid
    }

    /// Find the index of a thread by ID
    fn find_index(&self, tid: ThreadId) -> Option<usize> {
        self.threads.iter().position(|t| t.tid() == tid)
    }

    /// Move the current thread to the back of the ready queue
    fn preempt_current(&mut self) {
        if let Some(idx) = self.current {
            if idx < self.threads.len() {
                let thread = &mut self.threads[idx];
                if thread.state() == ThreadState::Running {
                    thread.set_state(ThreadState::Ready);
                    self.ready_queue.push_back(idx);
                }
            }
            self.current = None;
        }
    }

    /// Schedule the next thread
    fn schedule_next(&mut self) -> Option<usize> {
        while let Some(idx) = self.ready_queue.pop_front() {
            if idx < self.threads.len() {
                let thread = &mut self.threads[idx];
                if thread.state() == ThreadState::Ready {
                    thread.set_state(ThreadState::Running);
                    self.current = Some(idx);
                    self.ticks_remaining = TIME_SLICE;
                    return Some(idx);
                }
            }
        }
        None
    }

    /// Yield the current thread's time slice
    pub fn yield_current(&mut self) {
        self.ticks_remaining = 0;
    }

    /// Block the current thread
    pub fn block_current(&mut self) {
        if let Some(idx) = self.current {
            if idx < self.threads.len() {
                self.threads[idx].set_state(ThreadState::Blocked);
            }
            self.current = None;
        }
    }

    /// Unblock a thread (make it ready to run)
    pub fn unblock(&mut self, tid: ThreadId) {
        if let Some(idx) = self.find_index(tid) {
            let thread = &mut self.threads[idx];
            if thread.state() == ThreadState::Blocked {
                thread.set_state(ThreadState::Ready);
                self.ready_queue.push_back(idx);
            }
        }
    }

    /// Get the current thread's context pointer (for switching)
    pub fn current_context_ptr(&mut self) -> Option<*mut C> {
        self.current.map(|idx| self.threads[idx].context_mut() as *mut C)
    }

    /// Get the next thread's context pointer (for switching)
    pub fn next_context_ptr(&self) -> Option<*const C> {
        self.current.map(|idx| self.threads[idx].context() as *const C)
    }
}

impl<C: Context> Default for RoundRobinScheduler<C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C: Context> Scheduler for RoundRobinScheduler<C> {
    type Thread = KernelThread<C>;

    fn add(&mut self, thread: Self::Thread) {
        let idx = self.threads.len();
        self.threads.push(thread);

        // If the thread is ready, add it to the ready queue
        if self.threads[idx].state() == ThreadState::Ready {
            self.ready_queue.push_back(idx);
        }
    }

    fn remove(&mut self, tid: ThreadId) -> Option<Self::Thread> {
        let idx = self.find_index(tid)?;

        // Can't remove the currently running thread
        if self.current == Some(idx) {
            return None;
        }

        // Remove from ready queue if present
        self.ready_queue.retain(|&i| i != idx);

        // Adjust indices in ready queue for threads after this one
        for i in self.ready_queue.iter_mut() {
            if *i > idx {
                *i -= 1;
            }
        }

        // Adjust current index if needed
        if let Some(ref mut current) = self.current {
            if *current > idx {
                *current -= 1;
            }
        }

        Some(self.threads.remove(idx))
    }

    fn next(&mut self) -> Option<&mut Self::Thread> {
        // If no current thread or time slice expired, schedule next
        if self.current.is_none() {
            self.schedule_next();
        }

        self.current.map(move |idx| &mut self.threads[idx])
    }

    fn get(&self, tid: ThreadId) -> Option<&Self::Thread> {
        self.find_index(tid).map(|idx| &self.threads[idx])
    }

    fn get_mut(&mut self, tid: ThreadId) -> Option<&mut Self::Thread> {
        self.find_index(tid).map(move |idx| &mut self.threads[idx])
    }

    fn current(&self) -> Option<&Self::Thread> {
        self.current.map(|idx| &self.threads[idx])
    }

    fn current_mut(&mut self) -> Option<&mut Self::Thread> {
        self.current.map(move |idx| &mut self.threads[idx])
    }

    fn set_current(&mut self, tid: ThreadId) {
        if let Some(idx) = self.find_index(tid) {
            self.current = Some(idx);
            self.threads[idx].set_state(ThreadState::Running);
            self.ticks_remaining = TIME_SLICE;
        }
    }

    fn tick(&mut self) -> bool {
        if self.current.is_none() {
            // No current thread, try to schedule one
            return self.schedule_next().is_some();
        }

        self.ticks_remaining = self.ticks_remaining.saturating_sub(1);

        if self.ticks_remaining == 0 {
            // Time slice expired, preempt
            self.preempt_current();
            self.schedule_next();
            true
        } else {
            false
        }
    }

    fn len(&self) -> usize {
        self.threads.len()
    }
}
