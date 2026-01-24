//! Real-time scheduling class
//!
//! Implements FIFO and Round-Robin real-time scheduling policies.
//! RT tasks always preempt fair (CFS) tasks.

extern crate alloc;

use alloc::collections::VecDeque;
use sched_traits::{Pid, RunQueueOps, SchedClass, SchedPolicy, RT_PRIO_MAX};

/// Number of RT priority levels (1-99)
const RT_PRIO_LEVELS: usize = 99;

/// Real-time run queue
///
/// Contains per-priority queues and a bitmap for O(1) lookup of
/// the highest priority non-empty queue.
pub struct RtRunQueue {
    /// Per-priority FIFO queues (index 0 = priority 1, index 98 = priority 99)
    queues: [VecDeque<Pid>; RT_PRIO_LEVELS],
    /// Bitmap of non-empty queues (bit N set = queue N has tasks)
    /// Lower bits = lower priority
    bitmap: u128,
    /// Number of RT tasks in this run queue
    nr_running: u32,
}

impl RtRunQueue {
    /// Create a new empty RT run queue
    pub fn new() -> Self {
        Self {
            queues: core::array::from_fn(|_| VecDeque::new()),
            bitmap: 0,
            nr_running: 0,
        }
    }

    /// Enqueue a task with the given priority
    ///
    /// Priority is 1-99 (higher = more important)
    pub fn enqueue(&mut self, pid: Pid, priority: u8) {
        let idx = priority_to_index(priority);
        self.queues[idx].push_back(pid);
        self.bitmap |= 1u128 << idx;
        self.nr_running += 1;
    }

    /// Dequeue a specific task
    pub fn dequeue(&mut self, pid: Pid, priority: u8) -> bool {
        let idx = priority_to_index(priority);
        if let Some(pos) = self.queues[idx].iter().position(|&p| p == pid) {
            self.queues[idx].remove(pos);
            if self.queues[idx].is_empty() {
                self.bitmap &= !(1u128 << idx);
            }
            self.nr_running -= 1;
            true
        } else {
            false
        }
    }

    /// Pick the highest priority task
    ///
    /// Returns the PID of the highest priority runnable task, or None
    /// if no RT tasks are runnable.
    pub fn pick_next(&mut self) -> Option<Pid> {
        if self.bitmap == 0 {
            return None;
        }
        // Find highest priority (highest bit set)
        let idx = (127 - self.bitmap.leading_zeros()) as usize;
        self.queues[idx].front().copied()
    }

    /// Remove and return the highest priority task
    pub fn pop_next(&mut self) -> Option<Pid> {
        if self.bitmap == 0 {
            return None;
        }
        let idx = (127 - self.bitmap.leading_zeros()) as usize;
        let pid = self.queues[idx].pop_front();
        if self.queues[idx].is_empty() {
            self.bitmap &= !(1u128 << idx);
        }
        if pid.is_some() {
            self.nr_running -= 1;
        }
        pid
    }

    /// Move a task to the back of its priority queue (for RR time slice expiry)
    pub fn requeue_tail(&mut self, pid: Pid, priority: u8) {
        let idx = priority_to_index(priority);
        // Remove from current position and add to back
        if let Some(pos) = self.queues[idx].iter().position(|&p| p == pid) {
            self.queues[idx].remove(pos);
            self.queues[idx].push_back(pid);
        }
    }

    /// Get the number of RT tasks
    pub fn nr_running(&self) -> u32 {
        self.nr_running
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.nr_running == 0
    }

    /// Get highest priority of runnable tasks (1-99), or 0 if empty
    pub fn highest_priority(&self) -> u8 {
        if self.bitmap == 0 {
            0
        } else {
            index_to_priority((127 - self.bitmap.leading_zeros()) as usize)
        }
    }
}

impl Default for RtRunQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert priority (1-99) to array index (0-98)
#[inline]
fn priority_to_index(priority: u8) -> usize {
    (priority.clamp(1, RT_PRIO_MAX) - 1) as usize
}

/// Convert array index (0-98) to priority (1-99)
#[inline]
fn index_to_priority(idx: usize) -> u8 {
    (idx + 1) as u8
}

/// Real-time scheduling class
///
/// Handles SCHED_FIFO and SCHED_RR policies.
pub struct RtSchedClass;

impl RtSchedClass {
    /// Create a new RT scheduling class
    pub const fn new() -> Self {
        Self
    }
}

impl SchedClass for RtSchedClass {
    fn name(&self) -> &'static str {
        "rt"
    }

    fn enqueue_task(&self, rq: &mut dyn RunQueueOps, pid: Pid) {
        // Only enqueue if it's actually an RT task
        if let Some(policy) = rq.get_task_policy(pid) {
            if policy.is_realtime() {
                rq.inc_nr_running();
            }
        }
    }

    fn dequeue_task(&self, rq: &mut dyn RunQueueOps, pid: Pid) {
        if let Some(policy) = rq.get_task_policy(pid) {
            if policy.is_realtime() {
                rq.dec_nr_running();
            }
        }
    }

    fn pick_next_task(&self, _rq: &mut dyn RunQueueOps) -> Option<Pid> {
        // The actual picking is done by the RunQueue using RtRunQueue
        // This is called by the main scheduler loop
        None
    }

    fn put_prev_task(&self, rq: &mut dyn RunQueueOps, pid: Pid) {
        // Update accounting
        let now = rq.clock();
        if let Some(start) = rq.get_task_exec_start(pid) {
            let _delta = now.saturating_sub(start);
            // Accounting would be updated on the Task directly
        }
    }

    fn tick(&self, rq: &mut dyn RunQueueOps, pid: Pid) -> bool {
        let policy = match rq.get_task_policy(pid) {
            Some(p) => p,
            None => return false,
        };

        match policy {
            SchedPolicy::Fifo => {
                // FIFO tasks never get preempted by tick
                false
            }
            SchedPolicy::RoundRobin => {
                // Decrement time slice
                let slice = rq.get_task_time_slice(pid).unwrap_or(0);
                if slice > 0 {
                    rq.set_task_time_slice(pid, slice - 1);
                    if slice == 1 {
                        // Time slice expired, need to reschedule
                        // Reset time slice for next run
                        rq.set_task_time_slice(pid, SchedPolicy::RoundRobin.default_time_slice());
                        return true;
                    }
                }
                false
            }
            _ => false,
        }
    }

    fn check_preempt_curr(&self, rq: &dyn RunQueueOps, waking: Pid, curr: Pid) -> bool {
        let waking_policy = match rq.get_task_policy(waking) {
            Some(p) => p,
            None => return false,
        };

        // Only RT tasks handled by this class
        if !waking_policy.is_realtime() {
            return false;
        }

        let waking_prio = rq.get_task_rt_priority(waking).unwrap_or(0);

        // Check current task
        if let Some(curr_policy) = rq.get_task_policy(curr) {
            if curr_policy.is_realtime() {
                // Both RT: compare priorities
                let curr_prio = rq.get_task_rt_priority(curr).unwrap_or(0);
                waking_prio > curr_prio
            } else {
                // Waking is RT, current is not - always preempt
                true
            }
        } else {
            // Current task not found, preempt
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rt_queue_basic() {
        let mut rq = RtRunQueue::new();
        assert!(rq.is_empty());

        rq.enqueue(1, 50);
        rq.enqueue(2, 60);
        rq.enqueue(3, 40);

        assert_eq!(rq.nr_running(), 3);
        assert_eq!(rq.highest_priority(), 60);

        // Should pick highest priority first
        assert_eq!(rq.pick_next(), Some(2));
        assert_eq!(rq.pop_next(), Some(2));

        assert_eq!(rq.pick_next(), Some(1));
        assert_eq!(rq.pop_next(), Some(1));

        assert_eq!(rq.pick_next(), Some(3));
    }

    #[test]
    fn test_rt_queue_same_priority() {
        let mut rq = RtRunQueue::new();

        rq.enqueue(1, 50);
        rq.enqueue(2, 50);
        rq.enqueue(3, 50);

        // FIFO order at same priority
        assert_eq!(rq.pop_next(), Some(1));
        assert_eq!(rq.pop_next(), Some(2));
        assert_eq!(rq.pop_next(), Some(3));
    }

    #[test]
    fn test_rt_queue_requeue() {
        let mut rq = RtRunQueue::new();

        rq.enqueue(1, 50);
        rq.enqueue(2, 50);
        rq.enqueue(3, 50);

        // Requeue task 1 to back
        rq.requeue_tail(1, 50);

        // Should now be 2, 3, 1
        assert_eq!(rq.pop_next(), Some(2));
        assert_eq!(rq.pop_next(), Some(3));
        assert_eq!(rq.pop_next(), Some(1));
    }
}
