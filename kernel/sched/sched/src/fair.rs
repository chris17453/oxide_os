//! Fair (CFS-like) scheduling class
//!
//! Implements Completely Fair Scheduler semantics using virtual runtime (vruntime).
//! Tasks with lower vruntime are scheduled first, and vruntime accumulates
//! faster for lower-weight (higher nice) tasks.
//!
//! — GraveShift: Fixed-capacity min-heap, zero dynamic allocation. The old
//! BinaryHeap triggered Vec::grow_one → realloc → HEAP_ALLOCATOR.lock() inside
//! timer ISR context, permanently deadlocking when the interrupted task held
//! the heap lock. This implementation uses a pre-allocated array with manual
//! sift operations. Every enqueue, dequeue, and pick is ISR-safe by construction.

use sched_traits::{NICE_0_WEIGHT, Pid, RunQueueOps, SchedClass, TICK_NS};

/// Minimum scheduling granularity in nanoseconds
/// Tasks run for at least this long before being preempted
const SCHED_MIN_GRANULARITY_NS: u64 = 1_000_000; // 1ms

/// Target scheduling latency in nanoseconds
/// This is the period over which we try to give each task its fair share
const SCHED_LATENCY_NS: u64 = 6_000_000; // 6ms

/// Wakeup preemption granularity
/// A waking task preempts if its vruntime is this much smaller than current
const WAKEUP_GRANULARITY_NS: u64 = 1_000_000; // 1ms

/// — GraveShift: Maximum CFS tasks per CPU in the fixed-capacity heap.
/// Each CfsEntry is 16 bytes, so 256 entries = 4KB per CPU. The kernel
/// won't have 256 runnable CFS tasks on a single CPU — if it does,
/// we drop the task with a serial warning instead of deadlocking the system.
const MAX_CFS_TASKS: usize = 256;

/// Entry in the CFS tree
#[derive(Clone, Copy, Debug)]
pub(crate) struct CfsEntry {
    /// Virtual runtime (used for ordering)
    pub(crate) vruntime: u64,
    /// Process ID
    pub(crate) pid: Pid,
}

impl CfsEntry {
    /// Sentinel for unoccupied heap slots
    const EMPTY: Self = Self {
        vruntime: u64::MAX,
        pid: 0,
    };
}

impl PartialEq for CfsEntry {
    fn eq(&self, other: &Self) -> bool {
        self.vruntime == other.vruntime && self.pid == other.pid
    }
}

impl Eq for CfsEntry {}

impl PartialOrd for CfsEntry {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CfsEntry {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        // — GraveShift: Natural ordering — lower vruntime is Less (min-heap root).
        // Lower PID wins ties for determinism.
        match self.vruntime.cmp(&other.vruntime) {
            core::cmp::Ordering::Equal => self.pid.cmp(&other.pid),
            ord => ord,
        }
    }
}

/// CFS run queue — fixed-capacity binary min-heap
///
/// — GraveShift: The heart of CFS scheduling. A binary min-heap ordered by
/// vruntime gives O(log n) enqueue/pop and O(1) peek. The fixed-size array
/// eliminates ALL dynamic allocation, making every operation ISR-safe.
/// Linux uses an intrusive red-black tree (zero-alloc by design). Our fixed
/// heap achieves the same ISR safety with simpler code and better cache locality.
pub struct CfsRunQueue {
    /// Fixed-capacity min-heap. heap[0] has the lowest vruntime.
    /// Valid entries occupy heap[0..len].
    heap: [CfsEntry; MAX_CFS_TASKS],
    /// Number of valid entries in the heap
    len: usize,
    /// Total weight of all tasks in the queue
    load_weight: u64,
    /// Minimum vruntime (floor for new tasks to prevent starvation)
    min_vruntime: u64,
    /// Number of fair tasks
    nr_running: u32,
}

impl CfsRunQueue {
    /// Create a new empty CFS run queue
    pub fn new() -> Self {
        Self {
            heap: [CfsEntry::EMPTY; MAX_CFS_TASKS],
            len: 0,
            load_weight: 0,
            min_vruntime: 0,
            nr_running: 0,
        }
    }

    // ─── Heap internals: zero allocation, ISR-safe ─────────────────────

    /// Restore min-heap property by moving element at `idx` toward the root.
    /// Called after insertion (new element at bottom may be smaller than parent).
    fn sift_up(&mut self, mut idx: usize) {
        while idx > 0 {
            let parent = (idx - 1) / 2;
            if self.heap[idx] < self.heap[parent] {
                self.heap.swap(idx, parent);
                idx = parent;
            } else {
                break;
            }
        }
    }

    /// Restore min-heap property by moving element at `idx` toward the leaves.
    /// Called after removal (replacement element at top may be larger than children).
    fn sift_down(&mut self, mut idx: usize) {
        loop {
            let left = 2 * idx + 1;
            let right = 2 * idx + 2;
            let mut smallest = idx;

            if left < self.len && self.heap[left] < self.heap[smallest] {
                smallest = left;
            }
            if right < self.len && self.heap[right] < self.heap[smallest] {
                smallest = right;
            }
            if smallest == idx {
                break;
            }
            self.heap.swap(idx, smallest);
            idx = smallest;
        }
    }

    // ─── Public API ────────────────────────────────────────────────────

    /// Enqueue a task
    ///
    /// The task's vruntime is adjusted relative to min_vruntime. Woken tasks
    /// receive a vruntime credit of SCHED_LATENCY_NS (one scheduling period),
    /// matching Linux CFS behavior. This ensures recently-woken tasks are
    /// scheduled within one latency period rather than being starved by tasks
    /// that accumulated no vruntime while sleeping.
    pub fn enqueue(&mut self, pid: Pid, vruntime: u64, weight: u64) -> u64 {
        // Give woken tasks a vruntime credit of one scheduling period.
        // This prevents starvation: without this, a woken task's vruntime
        // gets clamped to min_vruntime which may equal long-sleeping tasks,
        // causing the woken task to wait behind all of them.
        let adjusted_vruntime = vruntime.max(self.min_vruntime.saturating_sub(SCHED_LATENCY_NS));

        if self.len < MAX_CFS_TASKS {
            self.heap[self.len] = CfsEntry {
                vruntime: adjusted_vruntime,
                pid,
            };
            self.len += 1;
            self.sift_up(self.len - 1);
        } else {
            // — GraveShift: 256 CFS tasks on one CPU. Something is deeply
            // wrong, but deadlocking the system is worse than dropping a task.
            unsafe {
                arch_x86_64::serial::write_str_unsafe(
                    "[CFS-FATAL] heap full, cannot enqueue task\n",
                );
            }
        }

        self.load_weight = self.load_weight.saturating_add(weight);
        self.nr_running += 1;

        adjusted_vruntime
    }

    /// Dequeue a specific task by PID
    ///
    /// O(n) scan to find the task + O(log n) heap fix. Zero allocation.
    /// The old code did drain().collect() + filter().collect() — two Vec
    /// allocations that could deadlock in ISR context. Never again.
    pub fn dequeue(&mut self, pid: Pid, weight: u64) -> bool {
        let pos = match (0..self.len).find(|&i| self.heap[i].pid == pid) {
            Some(p) => p,
            None => return false,
        };

        self.len -= 1;

        if pos < self.len {
            // Move last element to the vacated position and fix heap
            self.heap[pos] = self.heap[self.len];
            self.heap[self.len] = CfsEntry::EMPTY;

            // The replacement element may need to go up or down
            if pos > 0 && self.heap[pos] < self.heap[(pos - 1) / 2] {
                self.sift_up(pos);
            } else {
                self.sift_down(pos);
            }
        } else {
            // Removed the last element — heap is already valid
            self.heap[self.len] = CfsEntry::EMPTY;
        }

        self.load_weight = self.load_weight.saturating_sub(weight);
        self.nr_running = self.nr_running.saturating_sub(1);
        true
    }

    /// Pick the task with minimum vruntime (peek, no removal)
    pub fn pick_next(&self) -> Option<Pid> {
        if self.len > 0 {
            Some(self.heap[0].pid)
        } else {
            None
        }
    }

    /// Remove and return the task with minimum vruntime
    pub fn pop_next(&mut self, weight: u64) -> Option<Pid> {
        if self.len == 0 {
            return None;
        }

        let min_pid = self.heap[0].pid;
        self.len -= 1;

        if self.len > 0 {
            // Move last element to root and sift down
            self.heap[0] = self.heap[self.len];
            self.heap[self.len] = CfsEntry::EMPTY;
            self.sift_down(0);
        } else {
            self.heap[0] = CfsEntry::EMPTY;
        }

        self.load_weight = self.load_weight.saturating_sub(weight);
        self.nr_running = self.nr_running.saturating_sub(1);
        Some(min_pid)
    }

    /// Update min_vruntime based on leftmost task
    ///
    /// — GraveShift: The running task was popped from the tree (on_rq=false),
    /// so its vruntime must NOT clamp min_vruntime downward. Otherwise a low-
    /// vruntime blocker (poll/nanosleep loop) holds min_vruntime back, starving
    /// any task whose vruntime ran ahead while it was actually executing. Linux
    /// CFS only uses curr->vruntime when curr->on_rq; we mirror that by using
    /// curr_vruntime only when the tree is empty.
    pub fn update_min_vruntime(&mut self, curr_vruntime: u64) {
        let new_min = if self.len > 0 {
            self.heap[0].vruntime
        } else {
            curr_vruntime
        };

        // min_vruntime only increases
        self.min_vruntime = self.min_vruntime.max(new_min);
    }

    /// Get the number of fair tasks
    pub fn nr_running(&self) -> u32 {
        self.nr_running
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.nr_running == 0
    }

    /// Get total load weight
    pub fn load_weight(&self) -> u64 {
        self.load_weight
    }

    /// Get minimum vruntime
    pub fn min_vruntime(&self) -> u64 {
        self.min_vruntime
    }

    /// Set minimum vruntime (used during initialization)
    pub fn set_min_vruntime(&mut self, vruntime: u64) {
        self.min_vruntime = vruntime;
    }

    /// Calculate the ideal runtime for a task based on its weight
    ///
    /// ideal_time = (task_weight / total_weight) * sched_latency
    pub fn calc_ideal_runtime(&self, weight: u64) -> u64 {
        if self.load_weight == 0 {
            return SCHED_LATENCY_NS;
        }

        // Calculate share with 64-bit precision
        let scaled = (weight as u128) * (SCHED_LATENCY_NS as u128);
        let ideal = (scaled / self.load_weight as u128) as u64;

        // Ensure at least minimum granularity
        ideal.max(SCHED_MIN_GRANULARITY_NS)
    }
}

impl Default for CfsRunQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// Fair (CFS) scheduling class
pub struct FairSchedClass;

impl FairSchedClass {
    /// Create a new fair scheduling class
    pub const fn new() -> Self {
        Self
    }

    /// Calculate vruntime delta from actual runtime
    ///
    /// delta_vruntime = delta_exec * (NICE_0_WEIGHT / task_weight)
    pub fn calc_vruntime_delta(delta_exec: u64, weight: u64) -> u64 {
        if weight == 0 {
            return delta_exec;
        }
        let scaled = (delta_exec as u128) * (NICE_0_WEIGHT as u128);
        (scaled / weight as u128) as u64
    }
}

impl SchedClass for FairSchedClass {
    fn name(&self) -> &'static str {
        "fair"
    }

    fn enqueue_task(&self, rq: &mut dyn RunQueueOps, pid: Pid) {
        if let Some(policy) = rq.get_task_policy(pid) {
            if policy.is_fair() {
                rq.inc_nr_running();
            }
        }
    }

    fn dequeue_task(&self, rq: &mut dyn RunQueueOps, pid: Pid) {
        if let Some(policy) = rq.get_task_policy(pid) {
            if policy.is_fair() {
                rq.dec_nr_running();
            }
        }
    }

    fn pick_next_task(&self, _rq: &mut dyn RunQueueOps) -> Option<Pid> {
        // Actual picking is done by the RunQueue using CfsRunQueue
        None
    }

    fn put_prev_task(&self, rq: &mut dyn RunQueueOps, pid: Pid) {
        // Update vruntime for the task that just stopped running
        let now = rq.clock();
        if let Some(start) = rq.get_task_exec_start(pid) {
            let delta = now.saturating_sub(start);
            let weight = rq.get_task_weight(pid).unwrap_or(NICE_0_WEIGHT);
            let vruntime_delta = Self::calc_vruntime_delta(delta, weight);

            if let Some(old_vruntime) = rq.get_task_vruntime(pid) {
                rq.set_task_vruntime(pid, old_vruntime.saturating_add(vruntime_delta));
            }
        }
    }

    fn tick(&self, rq: &mut dyn RunQueueOps, pid: Pid) -> bool {
        let policy = match rq.get_task_policy(pid) {
            Some(p) => p,
            None => return false,
        };

        if !policy.is_fair() {
            return false;
        }

        // Update vruntime
        let now = rq.clock();
        if let Some(start) = rq.get_task_exec_start(pid) {
            let delta = now.saturating_sub(start);
            let weight = rq.get_task_weight(pid).unwrap_or(NICE_0_WEIGHT);
            let vruntime_delta = Self::calc_vruntime_delta(delta, weight);

            if let Some(old_vruntime) = rq.get_task_vruntime(pid) {
                let new_vruntime = old_vruntime.saturating_add(vruntime_delta);
                rq.set_task_vruntime(pid, new_vruntime);

                // Update min_vruntime
                let min_vr = rq.min_vruntime();
                if new_vruntime > min_vr {
                    rq.set_min_vruntime(new_vruntime);
                }
            }

            // Reset exec_start for next period
            rq.set_task_exec_start(pid, now);

            // Check if we've run long enough
            // With a single tick, this is approximate
            return delta >= TICK_NS && rq.nr_running() > 1;
        }

        false
    }

    fn check_preempt_curr(&self, rq: &dyn RunQueueOps, waking: Pid, curr: Pid) -> bool {
        let waking_policy = match rq.get_task_policy(waking) {
            Some(p) => p,
            None => return false,
        };

        let curr_policy = match rq.get_task_policy(curr) {
            Some(p) => p,
            None => return true, // No current task, preempt
        };

        // Fair tasks don't preempt RT tasks
        if curr_policy.is_realtime() {
            return false;
        }

        // Fair task waking against fair current
        if waking_policy.is_fair() && curr_policy.is_fair() {
            let waking_vr = rq.get_task_vruntime(waking).unwrap_or(0);
            let curr_vr = rq.get_task_vruntime(curr).unwrap_or(0);

            // Preempt if waking task has significantly lower vruntime
            return waking_vr + WAKEUP_GRANULARITY_NS < curr_vr;
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cfs_queue_ordering() {
        let mut cfs = CfsRunQueue::new();

        // Add tasks with different vruntimes
        cfs.enqueue(1, 1000, 1024);
        cfs.enqueue(2, 500, 1024);
        cfs.enqueue(3, 2000, 1024);

        // Should pick lowest vruntime first
        assert_eq!(cfs.pick_next(), Some(2));
        cfs.pop_next(1024);

        assert_eq!(cfs.pick_next(), Some(1));
        cfs.pop_next(1024);

        assert_eq!(cfs.pick_next(), Some(3));
    }

    #[test]
    fn test_cfs_min_vruntime() {
        let mut cfs = CfsRunQueue::new();

        cfs.min_vruntime = 10_000_000; // 10ms

        // Task with low vruntime gets credit: min_vruntime - SCHED_LATENCY_NS
        // min_vruntime=10ms, SCHED_LATENCY_NS=6ms, so floor = 4ms
        let adjusted = cfs.enqueue(1, 500, 1024);
        assert_eq!(adjusted, 10_000_000 - SCHED_LATENCY_NS); // 4ms

        // Task with vruntime above the floor keeps its vruntime
        let adjusted2 = cfs.enqueue(2, 10_000_000 + 1000, 1024);
        assert_eq!(adjusted2, 10_000_000 + 1000);
    }

    #[test]
    fn test_cfs_wakeup_credit_saturating() {
        let mut cfs = CfsRunQueue::new();

        // When min_vruntime is small, saturating_sub prevents underflow
        cfs.min_vruntime = 1000;
        let adjusted = cfs.enqueue(1, 500, 1024);
        // min_vruntime(1000) - SCHED_LATENCY_NS(6_000_000) saturates to 0
        // max(500, 0) = 500
        assert_eq!(adjusted, 500);
    }

    #[test]
    fn test_vruntime_calc() {
        // Nice 0 task: vruntime delta equals real delta
        let delta = FairSchedClass::calc_vruntime_delta(1000, NICE_0_WEIGHT);
        assert_eq!(delta, 1000);

        // Higher weight (lower nice) task: vruntime grows slower
        let delta_high = FairSchedClass::calc_vruntime_delta(1000, NICE_0_WEIGHT * 2);
        assert!(delta_high < 1000);

        // Lower weight (higher nice) task: vruntime grows faster
        let delta_low = FairSchedClass::calc_vruntime_delta(1000, NICE_0_WEIGHT / 2);
        assert!(delta_low > 1000);
    }

    #[test]
    fn test_cfs_dequeue() {
        let mut cfs = CfsRunQueue::new();

        cfs.enqueue(1, 1000, 1024);
        cfs.enqueue(2, 500, 1024);
        cfs.enqueue(3, 2000, 1024);

        // Dequeue the middle task
        assert!(cfs.dequeue(1, 1024));
        assert_eq!(cfs.nr_running(), 2);

        // Remaining should be in order: 2 (vr=500), 3 (vr=2000)
        assert_eq!(cfs.pick_next(), Some(2));
        cfs.pop_next(1024);
        assert_eq!(cfs.pick_next(), Some(3));
    }

    #[test]
    fn test_cfs_dequeue_root() {
        let mut cfs = CfsRunQueue::new();

        cfs.enqueue(1, 500, 1024);
        cfs.enqueue(2, 1000, 1024);
        cfs.enqueue(3, 2000, 1024);

        // Dequeue the root (min vruntime)
        assert!(cfs.dequeue(1, 1024));

        // Next should be pid 2
        assert_eq!(cfs.pick_next(), Some(2));
    }

    #[test]
    fn test_cfs_dequeue_nonexistent() {
        let mut cfs = CfsRunQueue::new();

        cfs.enqueue(1, 1000, 1024);

        // Dequeue a pid that doesn't exist
        assert!(!cfs.dequeue(99, 1024));
        assert_eq!(cfs.nr_running(), 1);
    }

    #[test]
    fn test_cfs_heap_integrity_after_many_ops() {
        let mut cfs = CfsRunQueue::new();

        // Enqueue many tasks in reverse vruntime order
        for i in (1..=20).rev() {
            cfs.enqueue(i, (i as u64) * 100, 1024);
        }

        // Pop all — should come out in ascending vruntime order
        let mut prev_vr = 0u64;
        for _ in 0..20 {
            let pid = cfs.pop_next(1024).unwrap();
            let vr = (pid as u64) * 100;
            assert!(vr >= prev_vr, "heap violated: vr={} after prev={}", vr, prev_vr);
            prev_vr = vr;
        }

        assert!(cfs.is_empty());
    }
}
