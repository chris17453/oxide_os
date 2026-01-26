//! Fair (CFS-like) scheduling class
//!
//! Implements Completely Fair Scheduler semantics using virtual runtime (vruntime).
//! Tasks with lower vruntime are scheduled first, and vruntime accumulates
//! faster for lower-weight (higher nice) tasks.

extern crate alloc;

use alloc::collections::BinaryHeap;
use core::cmp::Reverse;
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

/// Entry in the CFS tree
#[derive(Clone, Copy, Debug)]
struct CfsEntry {
    /// Virtual runtime (used for ordering)
    vruntime: u64,
    /// Process ID
    pid: Pid,
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
        // Primary: lower vruntime is "greater" (we use Reverse for min-heap)
        // Secondary: lower PID wins ties for determinism
        match self.vruntime.cmp(&other.vruntime) {
            core::cmp::Ordering::Equal => self.pid.cmp(&other.pid),
            ord => ord,
        }
    }
}

/// CFS run queue
///
/// Implements a min-heap ordered by vruntime for O(log n) operations.
/// Linux uses a red-black tree, but a binary heap is simpler and sufficient.
pub struct CfsRunQueue {
    /// Min-heap of tasks ordered by vruntime
    tasks: BinaryHeap<Reverse<CfsEntry>>,
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
            tasks: BinaryHeap::new(),
            load_weight: 0,
            min_vruntime: 0,
            nr_running: 0,
        }
    }

    /// Enqueue a task
    ///
    /// The task's vruntime is adjusted to be at least min_vruntime to prevent
    /// newly awakened tasks from monopolizing the CPU.
    pub fn enqueue(&mut self, pid: Pid, vruntime: u64, weight: u64) -> u64 {
        // Adjust vruntime: new tasks start at min_vruntime
        let adjusted_vruntime = vruntime.max(self.min_vruntime);

        self.tasks.push(Reverse(CfsEntry {
            vruntime: adjusted_vruntime,
            pid,
        }));
        self.load_weight = self.load_weight.saturating_add(weight);
        self.nr_running += 1;

        adjusted_vruntime
    }

    /// Dequeue a specific task
    pub fn dequeue(&mut self, pid: Pid, weight: u64) -> bool {
        // This is O(n) unfortunately, but we need to remove arbitrary tasks
        // A production implementation would use a more sophisticated data structure
        let len_before = self.tasks.len();
        let old_tasks: alloc::vec::Vec<_> = self.tasks.drain().collect();
        self.tasks = old_tasks
            .into_iter()
            .filter(|Reverse(e)| e.pid != pid)
            .collect();

        if self.tasks.len() < len_before {
            self.load_weight = self.load_weight.saturating_sub(weight);
            self.nr_running = self.nr_running.saturating_sub(1);
            true
        } else {
            false
        }
    }

    /// Pick the task with minimum vruntime
    pub fn pick_next(&self) -> Option<Pid> {
        self.tasks.peek().map(|Reverse(e)| e.pid)
    }

    /// Remove and return the task with minimum vruntime
    pub fn pop_next(&mut self, weight: u64) -> Option<Pid> {
        let entry = self.tasks.pop()?;
        self.load_weight = self.load_weight.saturating_sub(weight);
        self.nr_running = self.nr_running.saturating_sub(1);
        Some(entry.0.pid)
    }

    /// Update min_vruntime based on leftmost task
    ///
    /// min_vruntime is monotonically increasing and tracks the minimum
    /// vruntime of all runnable tasks.
    pub fn update_min_vruntime(&mut self, curr_vruntime: u64) {
        let min_from_tree = self.tasks.peek().map(|Reverse(e)| e.vruntime);

        let new_min = match min_from_tree {
            Some(tree_min) => tree_min.min(curr_vruntime),
            None => curr_vruntime,
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

        cfs.min_vruntime = 1000;

        // New task should get min_vruntime if its vruntime is lower
        let adjusted = cfs.enqueue(1, 500, 1024);
        assert_eq!(adjusted, 1000);
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
}
