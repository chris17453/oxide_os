//! Core scheduler functions
//!
//! This module contains the main scheduling entry points:
//! - schedule() - Pick and switch to the next task
//! - wake_up() - Wake a sleeping task
//! - block() - Block the current task
//! - scheduler_tick() - Handle timer tick

extern crate alloc;

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use os_core::PhysAddr;
use sched_traits::{CpuSet, Pid, SchedPolicy, TICK_NS, TaskState};
use spin::Mutex;

use crate::group::SchedGroups;
use crate::runqueue::RunQueue;
use crate::task::Task;

/// Maximum number of CPUs supported
pub const MAX_CPUS: usize = 256;

/// Per-CPU run queues
static RUN_QUEUES: [Mutex<Option<RunQueue>>; MAX_CPUS] = {
    const INIT: Mutex<Option<RunQueue>> = Mutex::new(None);
    [INIT; MAX_CPUS]
};

/// Number of active CPUs
static ACTIVE_CPUS: AtomicU32 = AtomicU32::new(0);

/// Current CPU (per-CPU variable would be better, using 0 for single CPU for now)
static CURRENT_CPU: AtomicU32 = AtomicU32::new(0);

/// Global scheduling groups
static SCHED_GROUPS: Mutex<Option<SchedGroups>> = Mutex::new(None);

/// Global clock (nanoseconds since boot)
static GLOBAL_CLOCK: AtomicU64 = AtomicU64::new(0);

/// Initialize the scheduler for a CPU
///
/// Must be called once for each CPU before scheduling can begin.
/// The first call initializes the global structures.
pub fn init_cpu(cpu: u32, idle_pid: Pid) {
    let mut rq_slot = RUN_QUEUES[cpu as usize].lock();
    *rq_slot = Some(RunQueue::new(cpu, idle_pid));

    // First CPU initializes global structures
    if ACTIVE_CPUS.fetch_add(1, Ordering::SeqCst) == 0 {
        let mut groups = SCHED_GROUPS.lock();
        *groups = Some(SchedGroups::new());
    }
}

/// Get the current CPU ID
pub fn this_cpu() -> u32 {
    CURRENT_CPU.load(Ordering::Relaxed)
}

/// Set the current CPU ID (called during context switch on SMP)
pub fn set_this_cpu(cpu: u32) {
    CURRENT_CPU.store(cpu, Ordering::Relaxed);
}

/// Get the number of active CPUs
pub fn num_cpus() -> u32 {
    ACTIVE_CPUS.load(Ordering::Relaxed)
}

/// Get access to a run queue
fn with_rq<F, R>(cpu: u32, f: F) -> Option<R>
where
    F: FnOnce(&mut RunQueue) -> R,
{
    let mut rq_slot = RUN_QUEUES[cpu as usize].lock();
    rq_slot.as_mut().map(f)
}

/// Try to get access to a run queue (non-blocking)
///
/// Returns None if the lock is already held (avoids deadlock in interrupt context)
fn try_with_rq<F, R>(cpu: u32, f: F) -> Option<R>
where
    F: FnOnce(&mut RunQueue) -> R,
{
    match RUN_QUEUES[cpu as usize].try_lock() {
        Some(mut rq_slot) => rq_slot.as_mut().map(f),
        None => None, // Lock held, skip this operation
    }
}

/// Get the global clock
pub fn global_clock() -> u64 {
    GLOBAL_CLOCK.load(Ordering::Relaxed)
}

/// Update the global clock
pub fn update_clock(now: u64) {
    GLOBAL_CLOCK.store(now, Ordering::Relaxed);
}

/// Add a task to the scheduler
///
/// The task is added to the appropriate run queue based on its CPU affinity.
pub fn add_task(task: Task) {
    let cpu = select_task_rq(&task);

    with_rq(cpu, |rq| {
        rq.add_task(task);
    });
}

/// Remove a task from the scheduler
pub fn remove_task(pid: Pid) -> Option<Task> {
    // Try to find and remove from all run queues
    for cpu in 0..num_cpus() {
        if let Some(task) = with_rq(cpu, |rq| rq.remove_task(pid)) {
            return task;
        }
    }
    None
}

/// Select the best CPU for a task based on affinity and load
fn select_task_rq(task: &Task) -> u32 {
    let affinity = &task.cpu_affinity;
    let last_cpu = task.last_cpu;

    // 1. Try last CPU if allowed (cache affinity)
    if affinity.is_set(last_cpu) {
        return last_cpu;
    }

    // 2. Try current CPU if allowed
    let current = this_cpu();
    if affinity.is_set(current) {
        return current;
    }

    // 3. Find least loaded allowed CPU
    let mut best_cpu = affinity.first_set().unwrap_or(0);
    let mut best_load = u32::MAX;

    for cpu in 0..num_cpus() {
        if !affinity.is_set(cpu) {
            continue;
        }

        let load = with_rq(cpu, |rq| rq.nr_running()).unwrap_or(u32::MAX);
        if load < best_load {
            best_load = load;
            best_cpu = cpu;
        }
    }

    best_cpu
}

/// Wake up a sleeping task
///
/// Moves the task to TASK_RUNNING and enqueues it on an appropriate run queue.
/// May trigger preemption if the woken task has higher priority.
pub fn wake_up(pid: Pid) {
    let cpu = this_cpu();

    // Find the task and update its state
    let task_cpu = with_rq(cpu, |rq| {
        if let Some(task) = rq.get_task_mut(pid) {
            task.state = TaskState::TASK_RUNNING;
            rq.enqueue_task(pid);

            // Check if we should preempt current
            if let Some(curr_pid) = rq.curr() {
                if should_preempt(rq, pid, curr_pid) {
                    rq.set_need_resched(true);
                }
            }
            Some(cpu)
        } else {
            None
        }
    });

    // If task wasn't on current CPU's run queue, search others
    if task_cpu.is_none() {
        for other_cpu in 0..num_cpus() {
            if other_cpu == cpu {
                continue;
            }

            let found = with_rq(other_cpu, |rq| {
                if let Some(task) = rq.get_task_mut(pid) {
                    task.state = TaskState::TASK_RUNNING;
                    rq.enqueue_task(pid);
                    rq.set_need_resched(true);
                    true
                } else {
                    false
                }
            });

            if found == Some(true) {
                // TODO: Send IPI to other_cpu to trigger reschedule
                break;
            }
        }
    }
}

/// Check if a waking task should preempt the current task
fn should_preempt(rq: &RunQueue, waking: Pid, curr: Pid) -> bool {
    let waking_task = match rq.get_task(waking) {
        Some(t) => t,
        None => return false,
    };

    let curr_task = match rq.get_task(curr) {
        Some(t) => t,
        None => return true, // No current task
    };

    // RT always preempts non-RT
    if waking_task.policy.is_realtime() && !curr_task.policy.is_realtime() {
        return true;
    }

    // Higher RT priority preempts
    if waking_task.policy.is_realtime() && curr_task.policy.is_realtime() {
        return waking_task.rt_priority > curr_task.rt_priority;
    }

    // Any task preempts idle
    if curr_task.policy == SchedPolicy::Idle && waking_task.policy != SchedPolicy::Idle {
        return true;
    }

    // CFS: check vruntime
    if waking_task.policy.is_fair() && curr_task.policy.is_fair() {
        // Preempt if waking task has significantly lower vruntime
        return waking_task.vruntime + 1_000_000 < curr_task.vruntime;
    }

    false
}

/// Block the current task
///
/// Sets the task state and removes it from the run queue.
/// The task will not run until wake_up() is called.
pub fn block_current(state: TaskState) {
    let cpu = this_cpu();

    with_rq(cpu, |rq| {
        if let Some(curr_pid) = rq.curr() {
            if let Some(task) = rq.get_task_mut(curr_pid) {
                task.state = state;
            }
            rq.dequeue_task(curr_pid);
            rq.set_need_resched(true);
        }
    });
}

/// Yield the current task voluntarily
///
/// The task remains runnable but moves to the back of its queue.
pub fn yield_current() {
    let cpu = this_cpu();

    with_rq(cpu, |rq| {
        if let Some(curr_pid) = rq.curr() {
            // Put current task back (will re-enqueue it)
            rq.put_prev_task(curr_pid);
            rq.set_need_resched(true);
        }
    });
}

/// Handle a scheduler tick
///
/// Called from the timer interrupt handler.
/// Returns true if rescheduling is needed.
pub fn scheduler_tick() -> bool {
    let cpu = this_cpu();
    let now = GLOBAL_CLOCK.fetch_add(TICK_NS, Ordering::Relaxed) + TICK_NS;

    // Use try_with_rq to avoid deadlock if lock is held by main code
    // If lock is held, we just skip this tick - it's not critical
    try_with_rq(cpu, |rq| {
        rq.update_clock(now);
        rq.scheduler_tick()
    })
    .unwrap_or(false)
}

/// Pick the next task to run
///
/// Returns the PID of the next task to run. This does not perform
/// the actual context switch - that must be done by the caller.
pub fn pick_next_task() -> Option<Pid> {
    let cpu = this_cpu();

    with_rq(cpu, |rq| {
        // Put back the previous task
        if let Some(prev_pid) = rq.curr() {
            // Update its accounting
            let now = rq.clock();
            if let Some(task) = rq.get_task_mut(prev_pid) {
                let delta = task.account_stop(now);
                task.update_vruntime(delta);
            }

            // Re-enqueue if still runnable
            let should_requeue = rq
                .get_task(prev_pid)
                .map(|task| task.state.is_runnable() && !task.is_idle())
                .unwrap_or(false);
            if should_requeue {
                rq.put_prev_task(prev_pid);
            }
        }

        // Pop the next task (removes from run queue, sets on_rq = false)
        let next = rq.pop_next_task();
        let now = rq.clock();

        // Mark it as running
        if let Some(task) = rq.get_task_mut(next) {
            task.state = TaskState::TASK_RUNNING;
            task.last_cpu = cpu;
            task.exec_start = now;
        }

        rq.set_curr(Some(next));
        rq.set_need_resched(false);

        Some(next)
    })
    .flatten()
}

/// Get the currently running task on this CPU
pub fn current_pid() -> Option<Pid> {
    let cpu = this_cpu();
    with_rq(cpu, |rq| rq.curr()).flatten()
}

/// Check if reschedule is needed
pub fn need_resched() -> bool {
    let cpu = this_cpu();
    with_rq(cpu, |rq| rq.need_resched()).unwrap_or(false)
}

/// Set the reschedule flag
pub fn set_need_resched() {
    let cpu = this_cpu();
    with_rq(cpu, |rq| rq.set_need_resched(true));
}

/// Switch current task to a new task (for fork/exec manual switches)
///
/// This properly re-enqueues the previous task and sets the new current.
/// Must be called when doing manual context switches outside of pick_next_task.
pub fn switch_to(new_pid: Pid) {
    let cpu = this_cpu();

    with_rq(cpu, |rq| {
        // Re-enqueue the previous task if it exists and is runnable
        if let Some(prev_pid) = rq.curr() {
            let should_requeue = rq
                .get_task(prev_pid)
                .map(|task| task.state.is_runnable() && !task.is_idle())
                .unwrap_or(false);
            if should_requeue {
                rq.put_prev_task(prev_pid);
            }
        }

        // Dequeue the new task from the run queue (it's becoming current)
        rq.dequeue_task(new_pid);

        // Set the new task as current
        let now = rq.clock();
        if let Some(task) = rq.get_task_mut(new_pid) {
            task.state = TaskState::TASK_RUNNING;
            task.last_cpu = cpu;
            task.exec_start = now;
            task.on_rq = false; // Current task is not on the run queue
        }
        rq.set_curr(Some(new_pid));
        rq.set_need_resched(false);
    });
}

/// Get task state
pub fn get_task_state(pid: Pid) -> Option<TaskState> {
    for cpu in 0..num_cpus() {
        if let Some(state) = with_rq(cpu, |rq| rq.get_task(pid).map(|t| t.state)).flatten() {
            return Some(state);
        }
    }
    None
}

/// Get task context (for context switching)
pub fn get_task_context(pid: Pid) -> Option<crate::task::TaskContext> {
    for cpu in 0..num_cpus() {
        if let Some(ctx) = with_rq(cpu, |rq| rq.get_task(pid).map(|t| t.context.clone())).flatten()
        {
            return Some(ctx);
        }
    }
    None
}

/// Update task context (for context switching)
pub fn set_task_context(pid: Pid, context: crate::task::TaskContext) {
    for cpu in 0..num_cpus() {
        let found = with_rq(cpu, |rq| {
            if let Some(task) = rq.get_task_mut(pid) {
                task.context = context;
                true
            } else {
                false
            }
        });

        if found == Some(true) {
            break;
        }
    }
}

/// Get task PML4 physical address (for address space switching)
pub fn get_task_pml4(pid: Pid) -> Option<PhysAddr> {
    for cpu in 0..num_cpus() {
        if let Some(pml4) = with_rq(cpu, |rq| rq.get_task(pid).map(|t| t.pml4_phys)).flatten() {
            return Some(pml4);
        }
    }
    None
}

/// Get task kernel stack info (for context switching)
pub fn get_task_kernel_stack(pid: Pid) -> Option<(PhysAddr, usize)> {
    for cpu in 0..num_cpus() {
        if let Some(info) = with_rq(cpu, |rq| {
            rq.get_task(pid)
                .map(|t| (t.kernel_stack, t.kernel_stack_size))
        })
        .flatten()
        {
            return Some(info);
        }
    }
    None
}

/// Update task execution context after exec()
///
/// Ensures the scheduler's view of the task matches the fresh address space
/// created by exec (new CR3, entry point, stack, and user-mode register state).
pub fn update_task_exec_info(
    pid: Pid,
    pml4_phys: PhysAddr,
    entry_point: u64,
    user_stack_top: u64,
    mut context: crate::task::TaskContext,
) {
    // exec() always returns to user mode, so make sure CS/SS/IF are valid
    if context.cs == 0 {
        context.cs = 0x23; // user code selector
    }
    if context.ss == 0 {
        context.ss = 0x1B; // user data selector
    }
    context.rip = entry_point;
    context.rsp = user_stack_top;
    context.rflags |= 0x200; // ensure IF is set

    for cpu in 0..num_cpus() {
        let updated = with_rq(cpu, |rq| {
            if let Some(task) = rq.get_task_mut(pid) {
                task.pml4_phys = pml4_phys;
                task.entry_point = entry_point;
                task.user_stack_top = user_stack_top;
                task.context = context;
                true
            } else {
                false
            }
        });

        if updated == Some(true) {
            break;
        }
    }
}

/// Get all context switch info for a task in one call (more efficient)
pub fn get_task_switch_info(
    pid: Pid,
) -> Option<(crate::task::TaskContext, PhysAddr, PhysAddr, usize)> {
    for cpu in 0..num_cpus() {
        if let Some(info) = with_rq(cpu, |rq| {
            rq.get_task(pid).map(|t| {
                (
                    t.context.clone(),
                    t.pml4_phys,
                    t.kernel_stack,
                    t.kernel_stack_size,
                )
            })
        })
        .flatten()
        {
            return Some(info);
        }
    }
    None
}

/// Set task CPU affinity
pub fn set_affinity(pid: Pid, cpuset: CpuSet) {
    for cpu in 0..num_cpus() {
        let found = with_rq(cpu, |rq| {
            if let Some(task) = rq.get_task_mut(pid) {
                task.cpu_affinity = cpuset;
                true
            } else {
                false
            }
        });

        if found == Some(true) {
            // TODO: If task can no longer run on this CPU, migrate it
            break;
        }
    }
}

/// Get task CPU affinity
pub fn get_affinity(pid: Pid) -> Option<CpuSet> {
    for cpu in 0..num_cpus() {
        if let Some(affinity) =
            with_rq(cpu, |rq| rq.get_task(pid).map(|t| t.cpu_affinity)).flatten()
        {
            return Some(affinity);
        }
    }
    None
}

/// Set task scheduling policy
pub fn set_scheduler(pid: Pid, policy: SchedPolicy, priority: u8) {
    for cpu in 0..num_cpus() {
        let found = with_rq(cpu, |rq| {
            // Check if task exists and get its current state
            let was_on_rq = rq.get_task(pid).map(|t| t.on_rq).unwrap_or(false);

            if rq.get_task(pid).is_none() {
                return false;
            }

            // Dequeue from old class
            if was_on_rq {
                rq.dequeue_task(pid);
            }

            // Update policy
            if let Some(task) = rq.get_task_mut(pid) {
                task.set_policy(policy);
                if policy.is_realtime() {
                    task.set_rt_priority(priority);
                }
            }

            // Re-enqueue with new class
            if was_on_rq {
                rq.enqueue_task(pid);
            }

            true
        });

        if found == Some(true) {
            break;
        }
    }
}

/// Get task scheduling policy
pub fn get_scheduler(pid: Pid) -> Option<(SchedPolicy, u8)> {
    for cpu in 0..num_cpus() {
        if let Some(result) = with_rq(cpu, |rq| {
            rq.get_task(pid).map(|t| (t.policy, t.rt_priority))
        })
        .flatten()
        {
            return Some(result);
        }
    }
    None
}

/// Set task nice value
pub fn set_nice(pid: Pid, nice: i8) {
    for cpu in 0..num_cpus() {
        let found = with_rq(cpu, |rq| {
            if let Some(task) = rq.get_task_mut(pid) {
                task.set_nice(nice);
                true
            } else {
                false
            }
        });

        if found == Some(true) {
            break;
        }
    }
}

/// Get task nice value
pub fn get_nice(pid: Pid) -> Option<i8> {
    for cpu in 0..num_cpus() {
        if let Some(nice) = with_rq(cpu, |rq| rq.get_task(pid).map(|t| t.nice)).flatten() {
            return Some(nice);
        }
    }
    None
}

/// Move a task to a scheduling group
pub fn set_task_group(pid: Pid, group_id: u32) {
    let mut groups = SCHED_GROUPS.lock();
    if let Some(groups) = groups.as_mut() {
        groups.set_task_group(pid, group_id);
    }
}

/// Get a task's scheduling group
pub fn get_task_group(pid: Pid) -> Option<u32> {
    let groups = SCHED_GROUPS.lock();
    groups.as_ref().and_then(|g| g.get_task_group(pid))
}

/// Create a new task with default parameters
pub fn create_task(
    pid: Pid,
    ppid: Pid,
    kernel_stack: PhysAddr,
    kernel_stack_size: usize,
    pml4_phys: PhysAddr,
    entry_point: u64,
    user_stack_top: u64,
) -> Task {
    Task::new(
        pid,
        ppid,
        kernel_stack,
        kernel_stack_size,
        pml4_phys,
        entry_point,
        user_stack_top,
    )
}

/// Disable preemption for current task
pub fn preempt_disable() {
    let cpu = this_cpu();
    with_rq(cpu, |rq| {
        if let Some(curr_pid) = rq.curr() {
            if let Some(task) = rq.get_task_mut(curr_pid) {
                task.preempt_disable();
            }
        }
    });
}

/// Enable preemption for current task
pub fn preempt_enable() {
    let cpu = this_cpu();
    with_rq(cpu, |rq| {
        if let Some(curr_pid) = rq.curr() {
            if let Some(task) = rq.get_task_mut(curr_pid) {
                task.preempt_enable();
            }
        }
    });
}

/// Check if preemption is disabled
pub fn preempt_disabled() -> bool {
    let cpu = this_cpu();
    with_rq(cpu, |rq| {
        if let Some(curr_pid) = rq.curr() {
            rq.get_task(curr_pid)
                .map(|t| t.preempt_disabled())
                .unwrap_or(false)
        } else {
            false
        }
    })
    .unwrap_or(false)
}

/// Debug: Get current scheduler state
///
/// Returns (current_pid, nr_running, cfs_queue_size)
pub fn debug_state() -> (Option<Pid>, u32, u32) {
    let cpu = this_cpu();
    with_rq(cpu, |rq| {
        let curr = rq.curr();
        let nr_running = rq.nr_running();
        let cfs_count = rq.cfs_rq().nr_running();
        (curr, nr_running, cfs_count)
    })
    .unwrap_or((None, 0, 0))
}

// ============================================================================
// ProcessMeta accessor functions
// These functions provide access to process metadata through the scheduler
// ============================================================================

use alloc::sync::Arc;
use proc::ProcessMeta;

/// Get process metadata for a task by PID
///
/// Searches all CPU run queues to find the task and returns
/// a clone of its ProcessMeta Arc (if present).
pub fn get_task_meta(pid: Pid) -> Option<Arc<Mutex<ProcessMeta>>> {
    for cpu in 0..num_cpus() {
        if let Some(meta) = with_rq(cpu, |rq| {
            rq.get_task(pid).and_then(|t| t.meta.clone())
        }).flatten() {
            return Some(meta);
        }
    }
    None
}

/// Get process metadata for the current task
pub fn get_current_meta() -> Option<Arc<Mutex<ProcessMeta>>> {
    current_pid().and_then(get_task_meta)
}

/// Execute a closure with read access to current task's ProcessMeta
pub fn with_current_meta<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&ProcessMeta) -> R,
{
    let meta = get_current_meta()?;
    Some(f(&meta.lock()))
}

/// Execute a closure with write access to current task's ProcessMeta
pub fn with_current_meta_mut<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut ProcessMeta) -> R,
{
    let meta = get_current_meta()?;
    Some(f(&mut meta.lock()))
}

/// Get the children of a task
pub fn get_task_children(pid: Pid) -> Vec<Pid> {
    for cpu in 0..num_cpus() {
        if let Some(children) = with_rq(cpu, |rq| {
            rq.get_task(pid).map(|t| t.children.clone())
        }).flatten() {
            return children;
        }
    }
    Vec::new()
}

/// Add a child to a task
pub fn add_task_child(pid: Pid, child_pid: Pid) {
    for cpu in 0..num_cpus() {
        let found = with_rq(cpu, |rq| {
            if let Some(task) = rq.get_task_mut(pid) {
                task.add_child(child_pid);
                true
            } else {
                false
            }
        });
        if found == Some(true) {
            break;
        }
    }
}

/// Remove a child from a task
pub fn remove_task_child(pid: Pid, child_pid: Pid) {
    for cpu in 0..num_cpus() {
        let found = with_rq(cpu, |rq| {
            if let Some(task) = rq.get_task_mut(pid) {
                task.remove_child(child_pid);
                true
            } else {
                false
            }
        });
        if found == Some(true) {
            break;
        }
    }
}

/// Set exit status for a task
pub fn set_task_exit_status(pid: Pid, status: i32) {
    for cpu in 0..num_cpus() {
        let found = with_rq(cpu, |rq| {
            if let Some(task) = rq.get_task_mut(pid) {
                task.exit(status);
                true
            } else {
                false
            }
        });
        if found == Some(true) {
            break;
        }
    }
}

/// Get a task's ppid
pub fn get_task_ppid(pid: Pid) -> Option<Pid> {
    for cpu in 0..num_cpus() {
        if let Some(ppid) = with_rq(cpu, |rq| {
            rq.get_task(pid).map(|t| t.ppid)
        }).flatten() {
            return Some(ppid);
        }
    }
    None
}

/// Get exit status of a task (if zombie)
pub fn get_task_exit_status(pid: Pid) -> Option<i32> {
    for cpu in 0..num_cpus() {
        if let Some((state, status)) = with_rq(cpu, |rq| {
            rq.get_task(pid).map(|t| (t.state, t.exit_status))
        }).flatten() {
            if state == TaskState::TASK_ZOMBIE {
                return Some(status);
            }
            return None;
        }
    }
    None
}

/// Check if a task is waiting for a specific child
pub fn is_task_waiting_for(pid: Pid, child_pid: Pid) -> bool {
    for cpu in 0..num_cpus() {
        if let Some(waiting) = with_rq(cpu, |rq| {
            rq.get_task(pid).map(|t| t.is_waiting_for(child_pid))
        }).flatten() {
            return waiting;
        }
    }
    false
}

/// Set a task to wait for a child
pub fn set_task_waiting(pid: Pid, child_pid: i32) {
    for cpu in 0..num_cpus() {
        let found = with_rq(cpu, |rq| {
            if let Some(task) = rq.get_task_mut(pid) {
                task.wait_for_child(child_pid);
                true
            } else {
                false
            }
        });
        if found == Some(true) {
            break;
        }
    }
}

/// Clear a task's waiting state
pub fn clear_task_waiting(pid: Pid) {
    for cpu in 0..num_cpus() {
        let found = with_rq(cpu, |rq| {
            if let Some(task) = rq.get_task_mut(pid) {
                task.clear_waiting();
                true
            } else {
                false
            }
        });
        if found == Some(true) {
            break;
        }
    }
}

/// Create a task with ProcessMeta
pub fn create_task_with_meta(
    pid: Pid,
    ppid: Pid,
    kernel_stack: PhysAddr,
    kernel_stack_size: usize,
    pml4_phys: PhysAddr,
    entry_point: u64,
    user_stack_top: u64,
    meta: Arc<Mutex<ProcessMeta>>,
) -> Task {
    Task::new_with_meta(
        pid,
        ppid,
        kernel_stack,
        kernel_stack_size,
        pml4_phys,
        entry_point,
        user_stack_top,
        meta,
    )
}

/// Set ProcessMeta on an existing task
pub fn set_task_meta(pid: Pid, meta: Arc<Mutex<ProcessMeta>>) {
    for cpu in 0..num_cpus() {
        let found = with_rq(cpu, |rq| {
            if let Some(task) = rq.get_task_mut(pid) {
                task.set_meta(meta.clone());
                true
            } else {
                false
            }
        });
        if found == Some(true) {
            break;
        }
    }
}
