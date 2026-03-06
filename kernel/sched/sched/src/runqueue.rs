//! Per-CPU run queue — flat slot array edition
//!
//! — TorqueJax: The BTreeMap that was here? Dead. Buried. Gone. Every
//! context switch was paying O(log n) lookup + heap alloc per BTree node.
//! Now it's a fixed 256-slot array with a free-stack allocator and a global
//! PID_TO_SLOT table for O(1) lookup. Zero heap allocations on the hot path.
//! The scheduler thanks you. The CPU cache thanks you. The users won't
//! notice because that's how infrastructure works.

extern crate alloc;

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU16, Ordering};
use sched_traits::{NICE_0_WEIGHT, Pid, RunQueueOps, SchedPolicy, TaskState};

use crate::fair::CfsRunQueue;
use crate::rt::RtRunQueue;
use crate::task::Task;

/// — TorqueJax: Max tasks per run queue. 256 slots × ~400B per Task ≈ 100KB
/// per CPU. Generous for an OS that'll never run 256 tasks on a single core.
/// If you hit this limit, congratulations — you have bigger problems.
pub const MAX_TASKS_PER_RQ: usize = 256;

/// — TorqueJax: Sentinel for "no slot assigned." u16::MAX is out-of-range
/// for a 256-slot array, so it's the perfect "not here" marker.
const SLOT_NONE: u16 = u16::MAX;

/// — TorqueJax: Global PID-to-slot mapping. Given a PID, one atomic load
/// gets you the slot index in that PID's run queue. Combined with PID_TO_CPU
/// (in core.rs), the full lookup is: PID → CPU → RQ lock → slots[slot].
/// No tree traversal. No hash buckets. Just array indexing. Beautiful.
const MAX_PIDS: usize = 4096;
static PID_TO_SLOT: [AtomicU16; MAX_PIDS] = {
    const INIT: AtomicU16 = AtomicU16::new(SLOT_NONE);
    [INIT; MAX_PIDS]
};

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
    /// — TorqueJax: Fixed-size task storage. Each slot is Option<Task> —
    /// None means the slot is free and sitting on the free_stack. Index
    /// into this array via PID_TO_SLOT[pid] for O(1) access. No more
    /// BTreeMap, no more heap churn, no more cache-busting pointer chases.
    slots: Vec<Option<Task>>,
    /// — TorqueJax: Stack of free slot indices. Pop to allocate, push to
    /// free. O(1) both ways. The Vec is allocated once at init and never
    /// grows — capacity == MAX_TASKS_PER_RQ.
    free_stack: Vec<u16>,
    /// Need reschedule flag
    need_resched: bool,
}

impl RunQueue {
    /// Create a new run queue for the given CPU
    pub fn new(cpu: u32, idle_pid: Pid) -> Self {
        // — TorqueJax: Pre-allocate all slots as None. One heap allocation
        // for the lifetime of this CPU's scheduler. That's it. That's the
        // entire memory budget.
        let mut slots = Vec::with_capacity(MAX_TASKS_PER_RQ);
        slots.resize_with(MAX_TASKS_PER_RQ, || None);

        // — TorqueJax: Initialize free stack with all slot indices.
        // Order doesn't matter for correctness but 0..N means we fill
        // from the bottom, which is cache-friendly for small task counts.
        let free_stack: Vec<u16> = (0..MAX_TASKS_PER_RQ as u16).rev().collect();

        Self {
            cpu,
            curr: None,
            idle: idle_pid,
            nr_running: 0,
            rt_rq: RtRunQueue::new(),
            cfs_rq: CfsRunQueue::new(),
            clock: 0,
            slots,
            free_stack,
            need_resched: false,
        }
    }

    // ========================================================================
    // — TorqueJax: Slot accessors. The two most called functions in the
    // entire scheduler. One atomic load, one bounds check, one array index.
    // That's three instructions between you and your task. The BTreeMap
    // needed ~15 pointer dereferences for the same thing. Rest in peace.
    // ========================================================================

    /// O(1) task lookup by PID
    #[inline]
    fn slot_get(&self, pid: Pid) -> Option<&Task> {
        if (pid as usize) >= MAX_PIDS {
            return None;
        }
        let slot = PID_TO_SLOT[pid as usize].load(Ordering::Relaxed) as usize;
        if slot >= MAX_TASKS_PER_RQ {
            return None;
        }
        // — TorqueJax: Validate PID matches. Stale PID_TO_SLOT entries are
        // possible during the window between remove_task and PID reuse.
        // One comparison is cheap insurance against ghost tasks.
        self.slots[slot].as_ref().filter(|t| t.pid == pid)
    }

    /// O(1) mutable task lookup by PID
    #[inline]
    fn slot_get_mut(&mut self, pid: Pid) -> Option<&mut Task> {
        if (pid as usize) >= MAX_PIDS {
            return None;
        }
        let slot = PID_TO_SLOT[pid as usize].load(Ordering::Relaxed) as usize;
        if slot >= MAX_TASKS_PER_RQ {
            return None;
        }
        self.slots[slot].as_mut().filter(|t| t.pid == pid)
    }

    /// Allocate a free slot, returns slot index or None if full
    #[inline]
    fn alloc_slot(&mut self) -> Option<u16> {
        self.free_stack.pop()
    }

    /// Return a slot to the free pool
    #[inline]
    fn free_slot(&mut self, slot: u16) {
        self.free_stack.push(slot);
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
        // — TorqueJax: Linear scan through slots. Only called from diagnostic
        // paths (procfs, debug dump) — never on the hot path. O(256) worst
        // case, but most slots are empty so the branch predictor feasts.
        self.slots
            .iter()
            .filter_map(|slot| slot.as_ref().map(|t| t.pid))
            .collect()
    }

    /// Get a reference to a task
    pub fn get_task(&self, pid: Pid) -> Option<&Task> {
        self.slot_get(pid)
    }

    /// Get a mutable reference to a task
    pub fn get_task_mut(&mut self, pid: Pid) -> Option<&mut Task> {
        self.slot_get_mut(pid)
    }

    /// Add a task to this run queue
    pub fn add_task(&mut self, mut task: Task) {
        if task.start_time == 0 {
            task.start_time = os_core::now_ns();
        }
        let pid = task.pid;

        // — TorqueJax: Pop a free slot, shove the task in, record the mapping.
        // If the RQ is full... well, you've got 256 tasks on one CPU. Maybe
        // rethink your life choices. Or increase MAX_TASKS_PER_RQ. Your call.
        let slot = match self.alloc_slot() {
            Some(s) => s,
            None => {
                // — TorqueJax: RQ full. This is a critical condition but
                // panicking in the scheduler is worse than dropping a task.
                // Log it and bail — the task simply won't be scheduled.
                #[cfg(feature = "debug-sched")]
                os_log::eprintln!("[SCHED] CRITICAL: RQ {} full, dropping pid {}", self.cpu, pid);
                return;
            }
        };

        self.slots[slot as usize] = Some(task);
        if (pid as usize) < MAX_PIDS {
            PID_TO_SLOT[pid as usize].store(slot, Ordering::Relaxed);
        }

        self.enqueue_task(pid);
    }

    /// Remove a task from this run queue
    pub fn remove_task(&mut self, pid: Pid) -> Option<Task> {
        if (pid as usize) >= MAX_PIDS {
            return None;
        }
        let slot = PID_TO_SLOT[pid as usize].load(Ordering::Relaxed);
        if slot as usize >= MAX_TASKS_PER_RQ {
            return None;
        }

        // — TorqueJax: Take the task out of the slot, verify PID matches
        // (stale mapping defense), then return the slot to the free pool.
        let task = self.slots[slot as usize].take()?;
        if task.pid != pid {
            // — TorqueJax: Stale mapping — put it back, this isn't our task.
            self.slots[slot as usize] = Some(task);
            return None;
        }

        // If the task is still queued, remove it from class queues and fix
        // runnable accounting before dropping the slot mapping.
        if task.on_rq {
            if task.policy.is_realtime() {
                self.rt_rq.dequeue(pid, task.rt_priority);
            } else if task.policy.is_fair() {
                self.cfs_rq.dequeue(pid, task.weight);
            }
            self.nr_running = self.nr_running.saturating_sub(1);
        }

        // Clear the PID→slot mapping
        PID_TO_SLOT[pid as usize].store(SLOT_NONE, Ordering::Relaxed);
        self.free_slot(slot);

        Some(task)
    }

    /// Enqueue a task that was not on the run queue
    pub fn enqueue_task(&mut self, pid: Pid) {
        let (policy, rt_prio, vruntime, weight) = {
            let task = match self.slot_get(pid) {
                Some(t) => t,
                None => return,
            };
            if task.on_rq {
                return;
            }
            (task.policy, task.rt_priority, task.vruntime, task.weight)
        };

        if policy.is_realtime() {
            self.rt_rq.enqueue(pid, rt_prio);
            self.nr_running += 1;
        } else if policy.is_fair() {
            let adjusted_vr = self.cfs_rq.enqueue(pid, vruntime, weight);
            if let Some(t) = self.slot_get_mut(pid) {
                t.vruntime = adjusted_vr;
            }
            self.nr_running += 1;
        }

        if let Some(t) = self.slot_get_mut(pid) {
            t.on_rq = true;
        }
    }

    /// Dequeue a task from the run queue
    pub fn dequeue_task(&mut self, pid: Pid) {
        let (policy, rt_prio, weight, on_rq) = {
            let task = match self.slot_get(pid) {
                Some(t) => t,
                None => return,
            };
            (task.policy, task.rt_priority, task.weight, task.on_rq)
        };

        if !on_rq {
            return;
        }

        if policy.is_realtime() {
            self.rt_rq.dequeue(pid, rt_prio);
            self.nr_running = self.nr_running.saturating_sub(1);
        } else if policy.is_fair() {
            self.cfs_rq.dequeue(pid, weight);
            self.nr_running = self.nr_running.saturating_sub(1);
        }

        if let Some(t) = self.slot_get_mut(pid) {
            t.on_rq = false;
        }
    }

    /// Pick the next task to run
    ///
    /// Order: RT > Fair > Idle
    pub fn pick_next_task(&mut self) -> Pid {
        if let Some(pid) = self.rt_rq.pick_next() {
            return pid;
        }

        if let Some(pid) = self.cfs_rq.pick_next() {
            return pid;
        }

        self.idle
    }

    /// Remove the next task from the queue and return it
    ///
    /// The task remains in self.slots but is removed from the run queue.
    pub fn pop_next_task(&mut self) -> Pid {
        if let Some(pid) = self.rt_rq.pop_next() {
            if let Some(t) = self.slot_get_mut(pid) {
                t.on_rq = false;
            }
            self.nr_running = self.nr_running.saturating_sub(1);
            return pid;
        }

        if let Some(pid) = self.cfs_rq.pick_next() {
            let weight = self
                .slot_get(pid)
                .map(|t| t.weight)
                .unwrap_or(NICE_0_WEIGHT);
            if let Some(pid) = self.cfs_rq.pop_next(weight) {
                if let Some(t) = self.slot_get_mut(pid) {
                    t.on_rq = false;
                }
                self.nr_running = self.nr_running.saturating_sub(1);
                return pid;
            }
        }

        self.idle
    }

    /// Put the previously running task back
    pub fn put_prev_task(&mut self, pid: Pid) {
        let dominated = {
            let task = match self.slot_get(pid) {
                Some(t) => t,
                None => return,
            };
            task.policy == SchedPolicy::Idle || task.state.is_dead()
        };

        if dominated {
            return;
        }

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

    /// — ThreadRogue: Steal the lowest-priority CFS task from this run queue
    /// for migration to an idle CPU. Returns the full Task (removed from this RQ)
    /// or None if nothing is stealable.
    ///
    /// Rules:
    /// - Never steal the currently running task (it's mid-execution)
    /// - Never steal idle or RT tasks (idle is per-CPU, RT has strict affinity)
    /// - Only steal if nr_running > 1 (leave at least one task for this CPU)
    /// - Check CPU affinity — don't steal a task pinned to this CPU
    pub fn steal_task(&mut self, dest_cpu: u32) -> Option<Task> {
        if self.nr_running <= 1 {
            return None;
        }

        // — ThreadRogue: Find the CFS task with the highest vruntime (lowest
        // priority). That's the one that's been getting the LEAST cpu time
        // relative to its fair share — ironic, but it's also the one that
        // benefits MOST from running on an otherwise-idle CPU.
        let mut best_pid: Option<Pid> = None;
        let mut best_vruntime: u64 = 0;

        for slot in self.slots.iter() {
            if let Some(task) = slot {
                // — ThreadRogue: Skip the currently running task, idle, RT,
                // tasks not on the run queue, dead tasks, and pinned tasks.
                if Some(task.pid) == self.curr {
                    continue;
                }
                if task.policy != SchedPolicy::Normal {
                    continue;
                }
                if !task.on_rq {
                    continue;
                }
                if task.state.is_dead() {
                    continue;
                }
                if !task.can_run_on(dest_cpu) {
                    continue;
                }
                if task.vruntime > best_vruntime || best_pid.is_none() {
                    best_vruntime = task.vruntime;
                    best_pid = Some(task.pid);
                }
            }
        }

        let victim_pid = best_pid?;
        self.remove_task(victim_pid)
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
            let task = match self.slot_get(curr_pid) {
                Some(t) => t,
                None => return false,
            };
            (task.policy, task.time_slice)
        };

        match policy {
            SchedPolicy::Fifo => {
                false
            }
            SchedPolicy::RoundRobin => {
                if time_slice > 0 {
                    if let Some(t) = self.slot_get_mut(curr_pid) {
                        t.time_slice -= 1;
                        if t.time_slice == 0 {
                            t.time_slice = SchedPolicy::RoundRobin.default_time_slice();
                            self.need_resched = true;
                            return true;
                        }
                    }
                }
                false
            }
            SchedPolicy::Normal | SchedPolicy::Batch => {
                // — GraveShift: CFS vruntime + preemption check.
                // Cache clock before mutable borrow — slot_get_mut borrows
                // all of self, so we can't touch self.clock inside the if-let.
                let clock = self.clock;
                let mut updated_vruntime = None;

                if let Some(t) = self.slot_get_mut(curr_pid) {
                    let delta = sched_traits::TICK_NS;
                    t.update_vruntime(delta);
                    t.vruntime_charged_this_tick = true;

                    if !in_blocking_wait {
                        t.sum_exec_runtime += delta;

                        if let Some(meta_arc) = t.meta.as_ref() {
                            if let Some(mut meta) = meta_arc.try_lock() {
                                meta.cpu_time_ns = t.sum_exec_runtime;
                            }
                        }
                    }

                    t.exec_start = clock;
                    updated_vruntime = Some(t.vruntime);
                }

                // — GraveShift: update_min_vruntime needs &mut cfs_rq, so
                // it must happen outside the slot_get_mut borrow scope.
                if let Some(vr) = updated_vruntime {
                    self.cfs_rq.update_min_vruntime(vr);
                }

                if let Some(next_pid) = self.cfs_rq.pick_next() {
                    if next_pid != curr_pid {
                        let curr_vr = self.slot_get(curr_pid).map(|t| t.vruntime).unwrap_or(0);
                        let next_vr = self.slot_get(next_pid).map(|t| t.vruntime).unwrap_or(0);

                        if next_vr + 1_000_000 < curr_vr {
                            self.need_resched = true;
                            return true;
                        }
                    }
                }
                false
            }
            SchedPolicy::Idle => {
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
        self.slot_get(pid).map(|t| t.state)
    }

    fn get_task_policy(&self, pid: Pid) -> Option<SchedPolicy> {
        self.slot_get(pid).map(|t| t.policy)
    }

    fn get_task_rt_priority(&self, pid: Pid) -> Option<u8> {
        self.slot_get(pid).map(|t| t.rt_priority)
    }

    fn get_task_vruntime(&self, pid: Pid) -> Option<u64> {
        self.slot_get(pid).map(|t| t.vruntime)
    }

    fn set_task_vruntime(&mut self, pid: Pid, vruntime: u64) {
        if let Some(t) = self.slot_get_mut(pid) {
            t.vruntime = vruntime;
        }
    }

    fn get_task_weight(&self, pid: Pid) -> Option<u64> {
        self.slot_get(pid).map(|t| t.weight)
    }

    fn get_task_time_slice(&self, pid: Pid) -> Option<u32> {
        self.slot_get(pid).map(|t| t.time_slice)
    }

    fn set_task_time_slice(&mut self, pid: Pid, slice: u32) {
        if let Some(t) = self.slot_get_mut(pid) {
            t.time_slice = slice;
        }
    }

    fn get_task_exec_start(&self, pid: Pid) -> Option<u64> {
        self.slot_get(pid).map(|t| t.exec_start)
    }

    fn set_task_exec_start(&mut self, pid: Pid, start: u64) {
        if let Some(t) = self.slot_get_mut(pid) {
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

    fn test_task(pid: Pid) -> Task {
        Task::new(
            pid,
            0,
            PhysAddr::new(0x1000),
            4096,
            PhysAddr::new(0x2000),
            0x400000,
            0x7fff0000,
        )
    }

    #[test]
    fn test_runqueue_basic() {
        let mut rq = RunQueue::new(0, 0);

        let mut task = test_task(1);
        task.policy = SchedPolicy::Normal;
        rq.add_task(task);

        assert_eq!(rq.pick_next_task(), 1);
    }

    #[test]
    fn test_rt_preempts_fair() {
        let mut rq = RunQueue::new(0, 0);

        let mut fair_task = test_task(1);
        fair_task.policy = SchedPolicy::Normal;
        rq.add_task(fair_task);

        let mut rt_task = test_task(2);
        rt_task.policy = SchedPolicy::Fifo;
        rt_task.rt_priority = 50;
        rq.add_task(rt_task);

        assert_eq!(rq.pick_next_task(), 2);
    }

    #[test]
    fn test_slot_allocation_and_removal() {
        let mut rq = RunQueue::new(0, 0);

        // — TorqueJax: Verify O(1) add/remove/lookup cycle
        let task = test_task(42);
        rq.add_task(task);

        assert!(rq.get_task(42).is_some());
        assert_eq!(rq.get_task(42).unwrap().pid, 42);

        let removed = rq.remove_task(42);
        assert!(removed.is_some());
        assert!(rq.get_task(42).is_none());
    }

    #[test]
    fn test_all_pids() {
        let mut rq = RunQueue::new(0, 0);

        for pid in 1..=5 {
            let mut t = test_task(pid);
            t.policy = SchedPolicy::Normal;
            rq.add_task(t);
        }

        let pids = rq.all_pids();
        assert_eq!(pids.len(), 5);
        for pid in 1..=5 {
            assert!(pids.contains(&pid));
        }
    }
}
