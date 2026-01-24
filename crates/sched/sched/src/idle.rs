//! Idle scheduling class
//!
//! The idle class is the lowest priority. Its tasks only run when
//! no other tasks (RT or fair) are runnable. Each CPU has an idle task.

use sched_traits::{Pid, RunQueueOps, SchedClass, SchedPolicy};

/// Idle scheduling class
///
/// This is the lowest priority scheduling class. Idle tasks only run
/// when there are no RT or fair tasks to run. Each CPU has exactly
/// one idle task that runs the idle loop (halting the CPU until an
/// interrupt arrives).
pub struct IdleSchedClass;

impl IdleSchedClass {
    /// Create a new idle scheduling class
    pub const fn new() -> Self {
        Self
    }
}

impl SchedClass for IdleSchedClass {
    fn name(&self) -> &'static str {
        "idle"
    }

    fn enqueue_task(&self, _rq: &mut dyn RunQueueOps, _pid: Pid) {
        // Idle tasks are never really "enqueued" - they're always there
        // Each CPU has exactly one idle task that's always available
    }

    fn dequeue_task(&self, _rq: &mut dyn RunQueueOps, _pid: Pid) {
        // Idle tasks are never really "dequeued" either
    }

    fn pick_next_task(&self, _rq: &mut dyn RunQueueOps) -> Option<Pid> {
        // The RunQueue handles returning the idle task when nothing else is available
        // This method is called by the main scheduler but idle tasks are handled specially
        None
    }

    fn put_prev_task(&self, _rq: &mut dyn RunQueueOps, _pid: Pid) {
        // Nothing to do for idle tasks - they don't accumulate runtime
        // that affects scheduling decisions
    }

    fn tick(&self, _rq: &mut dyn RunQueueOps, _pid: Pid) -> bool {
        // Idle tasks should always be preemptable (return true to check for work)
        // However, the actual preemption decision is made by checking if there
        // are other runnable tasks, not by this return value.
        true
    }

    fn check_preempt_curr(&self, rq: &dyn RunQueueOps, waking: Pid, _curr: Pid) -> bool {
        // Any non-idle task should preempt an idle task
        if let Some(policy) = rq.get_task_policy(waking) {
            policy != SchedPolicy::Idle
        } else {
            false
        }
    }
}

/// Per-CPU idle task holder
///
/// Stores the PID of the idle task for each CPU.
pub struct IdleTask {
    /// The idle task's PID
    pid: Pid,
}

impl IdleTask {
    /// Create a new idle task holder
    pub const fn new(pid: Pid) -> Self {
        Self { pid }
    }

    /// Get the idle task PID
    pub const fn pid(&self) -> Pid {
        self.pid
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_idle_class_name() {
        let idle = IdleSchedClass::new();
        assert_eq!(idle.name(), "idle");
    }
}
