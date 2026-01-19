//! SMP-aware scheduler extensions
//!
//! Provides per-CPU run queues for multi-processor scheduling.

use alloc::collections::VecDeque;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use sched_traits::{Context, Thread, ThreadId, ThreadState};
use spin::Mutex;

use crate::KernelThread;

/// Maximum number of CPUs supported
const MAX_CPUS: usize = 256;

/// Time slice in ticks before preemption
const TIME_SLICE: u32 = 10;

/// Per-CPU scheduler state
pub struct PerCpuScheduler {
    /// Queue of ready thread IDs for this CPU
    ready_queue: VecDeque<ThreadId>,
    /// Currently running thread ID
    current: Option<ThreadId>,
    /// Ticks remaining in current time slice
    ticks_remaining: u32,
    /// CPU affinity mask (which CPUs this scheduler can pull from)
    cpu_id: u32,
}

impl PerCpuScheduler {
    /// Create a new per-CPU scheduler for the given CPU
    pub const fn new(cpu_id: u32) -> Self {
        Self {
            ready_queue: VecDeque::new(),
            current: None,
            ticks_remaining: TIME_SLICE,
            cpu_id,
        }
    }

    /// Add a thread to this CPU's run queue
    pub fn enqueue(&mut self, tid: ThreadId) {
        self.ready_queue.push_back(tid);
    }

    /// Remove a specific thread from the run queue
    pub fn dequeue(&mut self, tid: ThreadId) -> bool {
        if let Some(pos) = self.ready_queue.iter().position(|&t| t == tid) {
            self.ready_queue.remove(pos);
            true
        } else {
            false
        }
    }

    /// Get the next thread to run
    pub fn next_thread(&mut self) -> Option<ThreadId> {
        self.ready_queue.pop_front()
    }

    /// Set the current thread
    pub fn set_current(&mut self, tid: Option<ThreadId>) {
        self.current = tid;
        if tid.is_some() {
            self.ticks_remaining = TIME_SLICE;
        }
    }

    /// Get the current thread ID
    pub fn current_tid(&self) -> Option<ThreadId> {
        self.current
    }

    /// Handle a timer tick, returns true if preemption should occur
    pub fn tick(&mut self) -> bool {
        if self.current.is_none() {
            return !self.ready_queue.is_empty();
        }

        self.ticks_remaining = self.ticks_remaining.saturating_sub(1);
        self.ticks_remaining == 0
    }

    /// Get number of threads in the ready queue
    pub fn queue_len(&self) -> usize {
        self.ready_queue.len()
    }

    /// Check if there are runnable threads
    pub fn has_runnable(&self) -> bool {
        !self.ready_queue.is_empty()
    }
}

/// Global SMP scheduler
///
/// Coordinates scheduling across all CPUs with load balancing.
pub struct SmpScheduler<C: Context> {
    /// Global thread storage
    threads: Mutex<Vec<KernelThread<C>>>,
    /// Per-CPU schedulers (accessed by CPU ID)
    per_cpu: [Mutex<PerCpuScheduler>; MAX_CPUS],
    /// Number of active CPUs
    active_cpus: AtomicU32,
    /// Next thread ID to allocate
    next_tid: AtomicUsize,
}

// We need to construct the array at runtime since const fn limitations
// prevent us from doing it at compile time
fn create_per_cpu_array() -> [Mutex<PerCpuScheduler>; MAX_CPUS] {
    core::array::from_fn(|i| Mutex::new(PerCpuScheduler::new(i as u32)))
}

impl<C: Context> SmpScheduler<C> {
    /// Create a new SMP scheduler
    pub fn new() -> Self {
        Self {
            threads: Mutex::new(Vec::new()),
            per_cpu: create_per_cpu_array(),
            active_cpus: AtomicU32::new(1), // BSP is always active
            next_tid: AtomicUsize::new(1),
        }
    }

    /// Initialize per-CPU scheduler for a specific CPU
    pub fn init_cpu(&self, cpu_id: u32) {
        let mut sched = self.per_cpu[cpu_id as usize].lock();
        sched.cpu_id = cpu_id;
        self.active_cpus.fetch_max(cpu_id + 1, Ordering::SeqCst);
    }

    /// Allocate a new thread ID
    pub fn alloc_tid(&self) -> ThreadId {
        self.next_tid.fetch_add(1, Ordering::SeqCst) as ThreadId
    }

    /// Add a new thread, optionally specifying CPU affinity
    pub fn add_thread(&self, thread: KernelThread<C>, preferred_cpu: Option<u32>) {
        let tid = thread.tid();
        let state = thread.state();

        // Store the thread
        {
            let mut threads = self.threads.lock();
            threads.push(thread);
        }

        // If ready, add to a CPU's run queue
        if state == ThreadState::Ready {
            let cpu = self.select_cpu(preferred_cpu);
            let mut sched = self.per_cpu[cpu as usize].lock();
            sched.enqueue(tid);
        }
    }

    /// Select the best CPU for a new thread
    fn select_cpu(&self, preferred: Option<u32>) -> u32 {
        let active = self.active_cpus.load(Ordering::Relaxed);

        // If preferred CPU is specified and valid, use it
        if let Some(cpu) = preferred {
            if cpu < active {
                return cpu;
            }
        }

        // Simple load balancing: find the CPU with the shortest queue
        let mut min_len = usize::MAX;
        let mut min_cpu = 0u32;

        for cpu in 0..active {
            let sched = self.per_cpu[cpu as usize].lock();
            let len = sched.queue_len();
            if len < min_len {
                min_len = len;
                min_cpu = cpu;
            }
        }

        min_cpu
    }

    /// Get the next thread to run on a specific CPU
    pub fn schedule(&self, cpu_id: u32) -> Option<ThreadId> {
        let mut sched = self.per_cpu[cpu_id as usize].lock();

        // First, try to get a thread from this CPU's queue
        if let Some(tid) = sched.next_thread() {
            sched.set_current(Some(tid));
            return Some(tid);
        }

        // If queue is empty, try to steal from other CPUs
        drop(sched);
        if let Some(tid) = self.try_steal_work(cpu_id) {
            let mut sched = self.per_cpu[cpu_id as usize].lock();
            sched.set_current(Some(tid));
            return Some(tid);
        }

        None
    }

    /// Try to steal a thread from another CPU's queue
    fn try_steal_work(&self, cpu_id: u32) -> Option<ThreadId> {
        let active = self.active_cpus.load(Ordering::Relaxed);

        // Round-robin through other CPUs
        for i in 1..active {
            let target = (cpu_id + i) % active;
            let mut target_sched = self.per_cpu[target as usize].lock();

            // Only steal if target has more than one thread
            if target_sched.queue_len() > 1 {
                if let Some(tid) = target_sched.next_thread() {
                    return Some(tid);
                }
            }
        }

        None
    }

    /// Handle a timer tick on a specific CPU
    pub fn tick(&self, cpu_id: u32) -> bool {
        let mut sched = self.per_cpu[cpu_id as usize].lock();
        sched.tick()
    }

    /// Yield the current thread on a CPU
    pub fn yield_thread(&self, cpu_id: u32) {
        let mut sched = self.per_cpu[cpu_id as usize].lock();
        if let Some(tid) = sched.current {
            sched.set_current(None);
            sched.enqueue(tid);
        }
    }

    /// Block the current thread on a CPU
    pub fn block_current(&self, cpu_id: u32) {
        let mut sched = self.per_cpu[cpu_id as usize].lock();
        if let Some(tid) = sched.current {
            // Update thread state
            let mut threads = self.threads.lock();
            if let Some(thread) = threads.iter_mut().find(|t| t.tid() == tid) {
                thread.set_state(ThreadState::Blocked);
            }
            sched.set_current(None);
        }
    }

    /// Wake up a blocked thread
    pub fn wake(&self, tid: ThreadId, preferred_cpu: Option<u32>) {
        let mut threads = self.threads.lock();
        if let Some(thread) = threads.iter_mut().find(|t| t.tid() == tid) {
            if thread.state() == ThreadState::Blocked {
                thread.set_state(ThreadState::Ready);
                let cpu = self.select_cpu(preferred_cpu);
                drop(threads);
                let mut sched = self.per_cpu[cpu as usize].lock();
                sched.enqueue(tid);
            }
        }
    }

    /// Get the current thread for a CPU
    pub fn current(&self, cpu_id: u32) -> Option<ThreadId> {
        let sched = self.per_cpu[cpu_id as usize].lock();
        sched.current
    }

    /// Get a reference to thread storage for direct access
    pub fn with_thread<F, R>(&self, tid: ThreadId, f: F) -> Option<R>
    where
        F: FnOnce(&KernelThread<C>) -> R,
    {
        let threads = self.threads.lock();
        threads.iter().find(|t| t.tid() == tid).map(f)
    }

    /// Get a mutable reference to thread storage for direct access
    pub fn with_thread_mut<F, R>(&self, tid: ThreadId, f: F) -> Option<R>
    where
        F: FnOnce(&mut KernelThread<C>) -> R,
    {
        let mut threads = self.threads.lock();
        threads.iter_mut().find(|t| t.tid() == tid).map(f)
    }

    /// Get total number of threads
    pub fn thread_count(&self) -> usize {
        self.threads.lock().len()
    }

    /// Get number of active CPUs
    pub fn cpu_count(&self) -> u32 {
        self.active_cpus.load(Ordering::Relaxed)
    }
}

impl<C: Context> Default for SmpScheduler<C> {
    fn default() -> Self {
        Self::new()
    }
}
