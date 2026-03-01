//! Per-CPU run queue
//!
//! The run queue is the central data structure for scheduling. Each CPU
//! has its own run queue containing tasks that can run on that CPU.

extern crate alloc;

use alloc::collections::BTreeMap;
use sched_traits::{NICE_0_WEIGHT, Pid, RunQueueOps, SchedPolicy, TaskState};

use crate::fair::CfsRunQueue;
use crate::rt::RtRunQueue;
use crate::task::Task;

/// Per-CPU run queue
///
/// Contains all scheduling state for a single CPU, including:
/// - RT run queue (priority-ordered)
/// - CFS run queue (vruntime-ordered)
/// - Idle task
/// - Currently running task
pub struct RunQueue {
    /// CPU ID
    cpu: u32,
    /// Currently running task (None if idle)
    curr: Option<Pid>,
    /// Idle task for this CPU
    idle: Pid,
    /// Number of runnable tasks (excludes idle)
    nr_running: u32,
    /// Real-time run queue
    rt_rq: RtRunQueue,
    /// CFS (fair) run queue
    pub(crate) cfs_rq: CfsRunQueue,
    /// Run queue clock (nanoseconds since boot)
    clock: u64,
    /// Tasks on this run queue (scheduler's view)
    pub(crate) tasks: BTreeMap<Pid, Task>,
    /// Need reschedule flag
    need_resched: bool,
}

impl RunQueue {
    /// Create a new run queue for the given CPU
    pub fn new(cpu: u32, idle_pid: Pid) -> Self {
        Self {
            cpu,
            curr: None,
            idle: idle_pid,
            nr_running: 0,
            rt_rq: RtRunQueue::new(),
            cfs_rq: CfsRunQueue::new(),
            clock: 0,
            tasks: BTreeMap::new(),
            need_resched: false,
        }
    }

    /// Get CPU ID
    pub fn cpu(&self) -> u32 {
        self.cpu
    }

    /// Get currently running task
    pub fn curr(&self) -> Option<Pid> {
        self.curr
    }

    /// Set currently running task
    pub fn set_curr(&mut self, pid: Option<Pid>) {
        self.curr = pid;
    }

    /// Get idle task
    pub fn idle(&self) -> Pid {
        self.idle
    }

    /// Get number of runnable tasks
    pub fn nr_running(&self) -> u32 {
        self.nr_running
    }

    /// Check if reschedule is needed
    pub fn need_resched(&self) -> bool {
        self.need_resched
    }

    /// Set reschedule flag
    pub fn set_need_resched(&mut self, need: bool) {
        self.need_resched = need;
    }

    /// Get all PIDs on this run queue
    pub fn all_pids(&self) -> alloc::vec::Vec<Pid> {
        self.tasks.keys().copied().collect()
    }

    /// Get a reference to a task
    pub fn get_task(&self, pid: Pid) -> Option<&Task> {
        self.tasks.get(&pid)
    }

    /// Get a mutable reference to a task
    pub fn get_task_mut(&mut self, pid: Pid) -> Option<&mut Task> {
        self.tasks.get_mut(&pid)
    }

    /// Add a task to this run queue
    pub fn add_task(&mut self, mut task: Task) {
        // Set start_time if not already set (0 means not set)
        if task.start_time == 0 {
            // Get current time in nanoseconds (100 Hz timer, 10ms per tick)
            task.start_time = arch_x86_64::timer_ticks() * 10_000_000;
        }
        let pid = task.pid;
        self.tasks.insert(pid, task);
        // Use enqueue_task for consistent accounting
        self.enqueue_task(pid);
    }

    /// Remove a task from this run queue
    pub fn remove_task(&mut self, pid: Pid) -> Option<Task> {
        let task = self.tasks.remove(&pid)?;

        if task.policy.is_realtime() {
            self.rt_rq.dequeue(pid, task.rt_priority);
        } else if task.policy.is_fair() {
            self.cfs_rq.dequeue(pid, task.weight);
        }

        Some(task)
    }

    /// Enqueue a task that was not on the run queue
    pub fn enqueue_task(&mut self, pid: Pid) {
        let (policy, rt_prio, vruntime, weight) = {
            let task = match self.tasks.get(&pid) {
                Some(t) => t,
                None => return,
            };
            if task.on_rq {
                return; // Already enqueued
            }
            (task.policy, task.rt_priority, task.vruntime, task.weight)
        };

        if policy.is_realtime() {
            self.rt_rq.enqueue(pid, rt_prio);
            self.nr_running += 1;
        } else if policy.is_fair() {
            let adjusted_vr = self.cfs_rq.enqueue(pid, vruntime, weight);
            if let Some(t) = self.tasks.get_mut(&pid) {
                t.vruntime = adjusted_vr;
            }
            self.nr_running += 1;
        }

        if let Some(t) = self.tasks.get_mut(&pid) {
            t.on_rq = true;
        }
    }

    /// Dequeue a task from the run queue
    pub fn dequeue_task(&mut self, pid: Pid) {
        let (policy, rt_prio, weight, on_rq) = {
            let task = match self.tasks.get(&pid) {
                Some(t) => t,
                None => return,
            };
            (task.policy, task.rt_priority, task.weight, task.on_rq)
        };

        if !on_rq {
            return; // Not on queue
        }

        if policy.is_realtime() {
            self.rt_rq.dequeue(pid, rt_prio);
            self.nr_running = self.nr_running.saturating_sub(1);
        } else if policy.is_fair() {
            self.cfs_rq.dequeue(pid, weight);
            self.nr_running = self.nr_running.saturating_sub(1);
        }

        if let Some(t) = self.tasks.get_mut(&pid) {
            t.on_rq = false;
        }
    }

    /// Pick the next task to run
    ///
    /// Order: RT > Fair > Idle
    pub fn pick_next_task(&mut self) -> Pid {
        // First check RT queue
        if let Some(pid) = self.rt_rq.pick_next() {
            return pid;
        }

        // Then check CFS queue
        if let Some(pid) = self.cfs_rq.pick_next() {
            return pid;
        }

        // Fall back to idle task
        self.idle
    }

    /// Remove the next task from the queue and return it
    ///
    /// The task remains in self.tasks but is removed from the run queue.
    pub fn pop_next_task(&mut self) -> Pid {
        // First check RT queue
        if let Some(pid) = self.rt_rq.pop_next() {
            if let Some(t) = self.tasks.get_mut(&pid) {
                t.on_rq = false;
            }
            self.nr_running = self.nr_running.saturating_sub(1);
            return pid;
        }

        // Then check CFS queue
        if let Some(pid) = self.cfs_rq.pick_next() {
            let weight = self
                .tasks
                .get(&pid)
                .map(|t| t.weight)
                .unwrap_or(NICE_0_WEIGHT);
            if let Some(pid) = self.cfs_rq.pop_next(weight) {
                if let Some(t) = self.tasks.get_mut(&pid) {
                    t.on_rq = false;
                }
                self.nr_running = self.nr_running.saturating_sub(1);
                return pid;
            }
        }

        // Fall back to idle task
        self.idle
    }

    /// Put the previously running task back
    pub fn put_prev_task(&mut self, pid: Pid) {
        // Check if task exists and should be re-enqueued
        let dominated = {
            let task = match self.tasks.get(&pid) {
                Some(t) => t,
                None => return,
            };
            // Don't re-enqueue idle task or dead tasks
            task.policy == SchedPolicy::Idle || task.state.is_dead()
        };

        if dominated {
            return;
        }

        // Use enqueue_task for consistent accounting
        self.enqueue_task(pid);
    }

    /// Update the run queue clock
    pub fn update_clock(&mut self, now: u64) {
        self.clock = now;
    }

    /// Get the run queue clock
    pub fn clock(&self) -> u64 {
        self.clock
    }

    /// Get CFS minimum vruntime
    pub fn min_vruntime(&self) -> u64 {
        self.cfs_rq.min_vruntime()
    }

    /// Set CFS minimum vruntime
    pub fn set_min_vruntime(&mut self, vruntime: u64) {
        self.cfs_rq.set_min_vruntime(vruntime);
    }

    /// Get RT run queue reference
    pub fn rt_rq(&self) -> &RtRunQueue {
        &self.rt_rq
    }

    /// Get RT run queue mutable reference
    pub fn rt_rq_mut(&mut self) -> &mut RtRunQueue {
        &mut self.rt_rq
    }

    /// Get CFS run queue reference
    pub fn cfs_rq(&self) -> &CfsRunQueue {
        &self.cfs_rq
    }

    /// Get CFS run queue mutable reference
    pub fn cfs_rq_mut(&mut self) -> &mut CfsRunQueue {
        &mut self.cfs_rq
    }

    /// Check if there are runnable tasks
    pub fn has_runnable(&self) -> bool {
        self.nr_running > 0
    }

    /// Handle a scheduler tick
    ///
    /// Returns true if preemption should occur.
    /// `in_blocking_wait` = true when the task is HLT-looping in a blocking
    /// syscall (poll, nanosleep, read). Don't charge CPU time in that case.
    ///
    /// NOTE: The caller (core.rs::scheduler_tick) already called update_clock()
    /// before this method. Do NOT advance self.clock here — that would double-count.
    pub fn scheduler_tick(&mut self, in_blocking_wait: bool) -> bool {
        let curr_pid = match self.curr {
            Some(pid) => pid,
            None => return false,
        };

        let (policy, time_slice) = {
            let task = match self.tasks.get(&curr_pid) {
                Some(t) => t,
                None => return false,
            };
            (task.policy, task.time_slice)
        };

        match policy {
            SchedPolicy::Fifo => {
                // FIFO tasks don't get preempted by timer
                false
            }
            SchedPolicy::RoundRobin => {
                // Decrement time slice
                if time_slice > 0 {
                    if let Some(t) = self.tasks.get_mut(&curr_pid) {
                        t.time_slice -= 1;
                        if t.time_slice == 0 {
                            // Reset for next time
                            t.time_slice = SchedPolicy::RoundRobin.default_time_slice();
                            self.need_resched = true;
                            return true;
                        }
                    }
                }
                false
            }
            SchedPolicy::Normal | SchedPolicy::Batch => {
                // CFS: update vruntime and check for preemption
                if let Some(t) = self.tasks.get_mut(&curr_pid) {
                    // — GraveShift: ALWAYS advance vruntime for CFS fairness.
                    // A task occupying the CPU (even just HLTing in a blocking
                    // wait) must pay CFS rent — otherwise its low vruntime
                    // permanently starves every other task. But only charge
                    // sum_exec_runtime (top/htop CPU%) when actively computing.
                    let delta = sched_traits::TICK_NS;
                    t.update_vruntime(delta);

                    if !in_blocking_wait {
                        t.sum_exec_runtime += delta;

                        // GraveShift: Sync authoritative scheduler accounting to ProcessMeta
                        if let Some(meta_arc) = t.meta.as_ref() {
                            if let Some(mut meta) = meta_arc.try_lock() {
                                meta.cpu_time_ns = t.sum_exec_runtime;
                            }
                        }
                    }

                    // Reset exec_start so pick_next_task's account_stop()
                    // doesn't re-count this same period.
                    t.exec_start = self.clock;

                    // Update min_vruntime
                    self.cfs_rq.update_min_vruntime(t.vruntime);
                }

                // Check if there's a better task to run
                if let Some(next_pid) = self.cfs_rq.pick_next() {
                    if next_pid != curr_pid {
                        let curr_vr = self.tasks.get(&curr_pid).map(|t| t.vruntime).unwrap_or(0);
                        let next_vr = self.tasks.get(&next_pid).map(|t| t.vruntime).unwrap_or(0);

                        // Preempt if next task has significantly lower vruntime
                        if next_vr + 1_000_000 < curr_vr {
                            // 1ms granularity
                            self.need_resched = true;
                            return true;
                        }
                    }
                }
                false
            }
            SchedPolicy::Idle => {
                // Always check for work when running idle task
                self.has_runnable()
            }
        }
    }
}

impl RunQueueOps for RunQueue {
    fn cpu(&self) -> u32 {
        self.cpu
    }

    fn nr_running(&self) -> u32 {
        self.nr_running
    }

    fn clock(&self) -> u64 {
        self.clock
    }

    fn update_clock(&mut self, now: u64) {
        self.clock = now;
    }

    fn get_task_state(&self, pid: Pid) -> Option<TaskState> {
        self.tasks.get(&pid).map(|t| t.state)
    }

    fn get_task_policy(&self, pid: Pid) -> Option<SchedPolicy> {
        self.tasks.get(&pid).map(|t| t.policy)
    }

    fn get_task_rt_priority(&self, pid: Pid) -> Option<u8> {
        self.tasks.get(&pid).map(|t| t.rt_priority)
    }

    fn get_task_vruntime(&self, pid: Pid) -> Option<u64> {
        self.tasks.get(&pid).map(|t| t.vruntime)
    }

    fn set_task_vruntime(&mut self, pid: Pid, vruntime: u64) {
        if let Some(t) = self.tasks.get_mut(&pid) {
            t.vruntime = vruntime;
        }
    }

    fn get_task_weight(&self, pid: Pid) -> Option<u64> {
        self.tasks.get(&pid).map(|t| t.weight)
    }

    fn get_task_time_slice(&self, pid: Pid) -> Option<u32> {
        self.tasks.get(&pid).map(|t| t.time_slice)
    }

    fn set_task_time_slice(&mut self, pid: Pid, slice: u32) {
        if let Some(t) = self.tasks.get_mut(&pid) {
            t.time_slice = slice;
        }
    }

    fn get_task_exec_start(&self, pid: Pid) -> Option<u64> {
        self.tasks.get(&pid).map(|t| t.exec_start)
    }

    fn set_task_exec_start(&mut self, pid: Pid, start: u64) {
        if let Some(t) = self.tasks.get_mut(&pid) {
            t.exec_start = start;
        }
    }

    fn min_vruntime(&self) -> u64 {
        self.cfs_rq.min_vruntime()
    }

    fn set_min_vruntime(&mut self, vruntime: u64) {
        self.cfs_rq.set_min_vruntime(vruntime);
    }

    fn inc_nr_running(&mut self) {
        self.nr_running += 1;
    }

    fn dec_nr_running(&mut self) {
        self.nr_running = self.nr_running.saturating_sub(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use os_core::PhysAddr;

    #[test]
    fn test_runqueue_basic() {
        let mut rq = RunQueue::new(0, 0);

        // Add a normal task
        let mut task = Task::new(1, PhysAddr::new(0x1000), 4096);
        task.policy = SchedPolicy::Normal;
        rq.add_task(task);

        assert_eq!(rq.pick_next_task(), 1);
    }

    #[test]
    fn test_rt_preempts_fair() {
        let mut rq = RunQueue::new(0, 0);

        // Add a fair task
        let mut fair_task = Task::new(1, PhysAddr::new(0x1000), 4096);
        fair_task.policy = SchedPolicy::Normal;
        rq.add_task(fair_task);

        // Add an RT task
        let mut rt_task = Task::new(2, PhysAddr::new(0x2000), 4096);
        rt_task.policy = SchedPolicy::Fifo;
        rt_task.rt_priority = 50;
        rq.add_task(rt_task);

        // RT task should be picked first
        assert_eq!(rq.pick_next_task(), 2);
    }
}
