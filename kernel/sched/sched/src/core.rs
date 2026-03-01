//! Core scheduler functions
//!
//! This module contains the main scheduling entry points:
//! - schedule() - Pick and switch to the next task
//! - wake_up() - Wake a sleeping task
//! - block() - Block the current task
//! - scheduler_tick() - Handle timer tick

extern crate alloc;

use alloc::vec::Vec;
use core::sync::atomic::{AtomicPtr, AtomicU32, AtomicU64, Ordering};
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

/// — GraveShift: Per-CPU tick counters for /proc/stat. Three slots per CPU:
/// [cpu*3+0] = user ticks, [cpu*3+1] = system ticks, [cpu*3+2] = idle ticks.
/// Incremented in scheduler_tick() from ISR context using try_with_rq, so
/// lock failure conservatively counts as idle. Values in nanoseconds.
static CPU_TICK_NS: [AtomicU64; MAX_CPUS * 3] = {
    const INIT: AtomicU64 = AtomicU64::new(0);
    [INIT; MAX_CPUS * 3]
};

/// Number of active CPUs
static ACTIVE_CPUS: AtomicU32 = AtomicU32::new(0);

/// NeonRoot: Arch-provided callback to get the current CPU's logical ID.
/// Without this, all CPUs stomp on a single global atomic — instant SMP crash.
/// The arch layer registers a function that reads the LAPIC ID and maps it to
/// a CPU index. Falls back to 0 (BSP) if no callback is registered.
static CPU_ID_FN: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

/// Register the architecture-specific CPU ID callback.
///
/// — NeonRoot: This MUST be called before APs start their timers.
/// The callback must be safe to call from interrupt context (no locks, no alloc).
pub fn register_cpu_id_fn(f: fn() -> u32) {
    CPU_ID_FN.store(f as *mut (), Ordering::Release);
}

/// Global scheduling groups
static SCHED_GROUPS: Mutex<Option<SchedGroups>> = Mutex::new(None);

/// Global clock (nanoseconds since boot)
static GLOBAL_CLOCK: AtomicU64 = AtomicU64::new(0);

/// Per-CPU current PID (lock-free, for interrupt handlers)
/// This shadows rq.curr but can be read without acquiring the run queue lock.
static CURRENT_PIDS: [AtomicU32; MAX_CPUS] = {
    const INIT: AtomicU32 = AtomicU32::new(u32::MAX); // MAX = no task (None)
    [INIT; MAX_CPUS]
};

/// Get current PID without lock (for interrupt handlers)
///
/// This is safe to call from any context, including interrupt handlers.
pub fn current_pid_lockfree() -> Option<Pid> {
    let cpu = this_cpu();
    let pid = CURRENT_PIDS[cpu as usize].load(Ordering::Relaxed);
    if pid == u32::MAX { None } else { Some(pid) }
}

/// Set current PID (lock-free update)
fn set_current_pid_lockfree(cpu: u32, pid: Option<Pid>) {
    let val = pid.unwrap_or(u32::MAX);
    CURRENT_PIDS[cpu as usize].store(val, Ordering::Relaxed);
}

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

    // Initialize lock-free current PID to idle task
    set_current_pid_lockfree(cpu, Some(idle_pid));
}

/// Get the current CPU's logical ID.
///
/// — NeonRoot: Uses the arch-provided callback to read the LAPIC ID and map
/// it to a CPU index. This is safe from any context (interrupt, normal, AP).
/// Falls back to 0 (BSP) if no callback is registered yet.
pub fn this_cpu() -> u32 {
    let ptr = CPU_ID_FN.load(Ordering::Acquire);
    if ptr.is_null() {
        return 0; // BSP default before callback is registered
    }
    let f: fn() -> u32 = unsafe { core::mem::transmute(ptr) };
    f()
}

/// Set the current CPU ID (legacy — now a no-op, CPU ID comes from LAPIC).
///
/// — NeonRoot: Kept for API compatibility during SMP bringup. The actual
/// CPU ID is determined by the arch callback registered via register_cpu_id_fn.
pub fn set_this_cpu(_cpu: u32) {
    // No-op: CPU ID is now derived from hardware (LAPIC ID)
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

/// — SableWire: Check if the RQ lock for this CPU is available (non-blocking probe).
/// Called from the timer ISR before attempting a context switch. If the interrupted
/// code was inside a scheduler operation (yield_current, block_current, etc.) the
/// lock is held and any blocking with_rq call would deadlock the ISR forever.
pub fn rq_lock_available() -> bool {
    let cpu = this_cpu();
    try_with_rq(cpu, |_| ()).is_some()
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
///
/// — GraveShift: Fast-path this_cpu() — zombie reap removes from the CPU the task died on.
pub fn remove_task(pid: Pid) -> Option<Task> {
    let cpu = this_cpu();
    if let Some(task) = with_rq(cpu, |rq| rq.remove_task(pid)) {
        if task.is_some() {
            return task;
        }
    }
    for other in 0..num_cpus() {
        if other == cpu {
            continue;
        }
        if let Some(task) = with_rq(other, |rq| rq.remove_task(pid)) {
            if task.is_some() {
                return task;
            }
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

/// Wake up a sleeping task (ISR-safe, non-blocking)
///
/// — GraveShift: Identical to wake_up() but uses try_with_rq to avoid
/// deadlock when the timer ISR interrupts code that already holds the
/// RQ spin lock. Returns true if the wake succeeded, false if the lock
/// was contended (caller should retry on the next tick).
pub fn try_wake_up(pid: Pid) -> bool {
    let cpu = this_cpu();

    // Try current CPU first (non-blocking)
    let result = try_with_rq(cpu, |rq| {
        if let Some(task) = rq.get_task_mut(pid) {
            task.state = TaskState::TASK_RUNNING;
            rq.enqueue_task(pid);
            if let Some(curr_pid) = rq.curr() {
                if should_preempt(rq, pid, curr_pid) {
                    rq.set_need_resched(true);
                }
            }
            true
        } else {
            false // Not on this CPU
        }
    });

    match result {
        Some(true) => return true,
        None => return false, // Lock contended — caller should retry
        Some(false) => {}     // Task not on this CPU, try others
    }

    // Try other CPUs (non-blocking)
    for other_cpu in 0..num_cpus() {
        if other_cpu == cpu {
            continue;
        }

        let found = try_with_rq(other_cpu, |rq| {
            if let Some(task) = rq.get_task_mut(pid) {
                task.state = TaskState::TASK_RUNNING;
                rq.enqueue_task(pid);
                rq.set_need_resched(true);
                true
            } else {
                false
            }
        });

        match found {
            Some(true) => {
                smp::ipi::send_reschedule(other_cpu);
                return true;
            }
            None => return false, // Lock contended on remote CPU
            Some(false) => {}     // Not on this CPU either
        }
    }

    false // Task not found anywhere
}

/// Wake up a sleeping task
///
/// Moves the task to TASK_RUNNING and enqueues it on an appropriate run queue.
/// May trigger preemption if the woken task has higher priority.
///
/// WARNING: Uses blocking with_rq — MUST NOT be called from ISR context.
/// Use try_wake_up() instead for timer interrupt handlers.
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
                // NeonRoot: Kick remote CPU to reschedule immediately (don't wait for timer tick)
                smp::ipi::send_reschedule(other_cpu);
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
/// `in_blocking_wait` is true when the current task is in a blocking syscall
/// (poll, nanosleep, read) doing HLT — it should not be charged as CPU time.
/// Returns true if rescheduling is needed.
pub fn scheduler_tick_ex(in_blocking_wait: bool) -> bool {
    let cpu = this_cpu();
    // — SableWire: SMP rule #1 — only BSP (CPU 0) advances the global clock.
    // With N CPUs all calling fetch_add, the clock ran N× too fast, corrupting
    // every timing calculation in the system. APs just read the current value.
    let now = if cpu == 0 {
        GLOBAL_CLOCK.fetch_add(TICK_NS, Ordering::Relaxed) + TICK_NS
    } else {
        GLOBAL_CLOCK.load(Ordering::Relaxed)
    };

    // — GraveShift: Classify this tick for /proc/stat accounting.
    // try_with_rq avoids deadlock if main code holds the lock.
    // Lock failure = we can't tell what's running, count as idle.
    let base = (cpu as usize) * 3;
    let resched = try_with_rq(cpu, |rq| {
        let is_idle = match rq.curr() {
            None => true,
            Some(pid) => rq
                .get_task(pid)
                .map(|t| t.policy == SchedPolicy::Idle)
                .unwrap_or(true),
        };
        // — GraveShift: A process HLT-looping in a blocking syscall (poll,
        // nanosleep, read) is NOT computing — don't charge it as user time.
        // kernel_preempt_ok == true means the task is waiting, not working.
        if is_idle || in_blocking_wait {
            CPU_TICK_NS[base + 2].fetch_add(TICK_NS, Ordering::Relaxed);
        } else {
            CPU_TICK_NS[base + 0].fetch_add(TICK_NS, Ordering::Relaxed);
        }
        rq.update_clock(now);
        rq.scheduler_tick(in_blocking_wait)
    });

    match resched {
        Some(r) => r,
        None => {
            // Lock held — count as idle conservatively
            CPU_TICK_NS[base + 2].fetch_add(TICK_NS, Ordering::Relaxed);
            false
        }
    }
}

/// Handle a scheduler tick (legacy wrapper, assumes active computation)
pub fn scheduler_tick() -> bool {
    scheduler_tick_ex(false)
}

/// Get per-CPU tick times in nanoseconds: (user_ns, system_ns, idle_ns)
///
/// — GraveShift: Lock-free read of atomic counters. Safe from any context.
pub fn get_cpu_times(cpu: u32) -> (u64, u64, u64) {
    let base = (cpu as usize) * 3;
    (
        CPU_TICK_NS[base + 0].load(Ordering::Relaxed),
        CPU_TICK_NS[base + 1].load(Ordering::Relaxed),
        CPU_TICK_NS[base + 2].load(Ordering::Relaxed),
    )
}

/// Pick the next task to run
///
/// Returns the PID of the next task to run. This does not perform
/// the actual context switch - that must be done by the caller.
pub fn pick_next_task() -> Option<Pid> {
    let cpu = this_cpu();

    with_rq(cpu, |rq| {
        // — GraveShift: Sync rq.clock from GLOBAL_CLOCK BEFORE accounting.
        // scheduler_tick_ex uses try_with_rq (non-blocking) — when the timer ISR
        // interrupts code that holds the RQ lock, try_with_rq fails and rq.clock
        // stays stale. Without this sync, account_stop() computes delta ≈ 0,
        // the task's vruntime never advances, and CFS starves every other task.
        // GLOBAL_CLOCK always advances (atomic fetch_add outside the lock).
        let fresh_now = GLOBAL_CLOCK.load(Ordering::Relaxed);
        rq.update_clock(fresh_now);

        // Put back the previous task
        if let Some(prev_pid) = rq.curr() {
            // Update its accounting
            let now = rq.clock();
            if let Some(task) = rq.get_task_mut(prev_pid) {
                let mut delta = task.account_stop(now);
                // — GraveShift: Sub-tick vruntime floor. With 10ms tick-based
                // accounting, a task that runs for <1 tick (e.g. servicemgr:
                // wake → usleep → HLT in microseconds) gets delta=0, so its
                // vruntime NEVER advances. Combined with the wakeup credit
                // (min_vruntime - SCHED_LATENCY), these rapid-sleepers permanently
                // starve every other task. Charging TICK_NS minimum ensures CFS
                // makes forward progress even without a TSC-based update_curr.
                // A full tick is the coarsest fair charge — we can't measure less.
                if delta == 0 && task.policy.is_fair() {
                    delta = TICK_NS;
                }
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
///
/// Uses try_with_rq (non-blocking) because this is called from the timer
/// ISR in scheduler_tick. Using the blocking with_rq would deadlock if
/// the interrupted code was holding the run queue lock.
pub fn need_resched() -> bool {
    let cpu = this_cpu();
    try_with_rq(cpu, |rq| rq.need_resched()).unwrap_or(false)
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

        // — GraveShift: Sync clock from GLOBAL_CLOCK (same stale-clock fix as pick_next_task)
        let fresh_now = GLOBAL_CLOCK.load(Ordering::Relaxed);
        rq.update_clock(fresh_now);
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

    // Update lock-free current PID for interrupt handlers
    set_current_pid_lockfree(cpu, Some(new_pid));

    // Restore FS base register for Thread-Local Storage (TLS)
    // This must be done on every context switch because FS_BASE is not saved/restored
    // by the CPU during interrupts (unlike general-purpose registers)
    if let Some(ctx) = get_task_context(new_pid) {
        if ctx.fs_base != 0 {
            unsafe {
                core::arch::asm!(
                    "mov rcx, 0xC0000100",  // IA32_FS_BASE MSR
                    "mov rax, {0}",          // Low 32 bits
                    "mov rdx, {0}",          // Copy for shift
                    "shr rdx, 32",           // High 32 bits
                    "wrmsr",
                    in(reg) ctx.fs_base,
                    out("rax") _,
                    out("rcx") _,
                    out("rdx") _,
                    options(nostack, preserves_flags)
                );
            }
        }
    }
}

/// Get task state
///
/// — GraveShift: Fast-path tries this_cpu() first. The current task is ALWAYS
/// on the local RQ, and with all tasks on CPU 0, this hits first try 99% of the time.
pub fn get_task_state(pid: Pid) -> Option<TaskState> {
    let cpu = this_cpu();
    if let Some(state) = with_rq(cpu, |rq| rq.get_task(pid).map(|t| t.state)).flatten() {
        return Some(state);
    }
    for other in 0..num_cpus() {
        if other == cpu {
            continue;
        }
        if let Some(state) = with_rq(other, |rq| rq.get_task(pid).map(|t| t.state)).flatten() {
            return Some(state);
        }
    }
    None
}

/// Get task context (for context switching)
///
/// — GraveShift: Fast-path this_cpu() — context switches always operate on local tasks.
pub fn get_task_context(pid: Pid) -> Option<crate::task::TaskContext> {
    let cpu = this_cpu();
    if let Some(ctx) = with_rq(cpu, |rq| rq.get_task(pid).map(|t| t.context.clone())).flatten() {
        return Some(ctx);
    }
    for other in 0..num_cpus() {
        if other == cpu {
            continue;
        }
        if let Some(ctx) =
            with_rq(other, |rq| rq.get_task(pid).map(|t| t.context.clone())).flatten()
        {
            return Some(ctx);
        }
    }
    None
}

/// Update task context (for context switching)
///
/// — GraveShift: Fast-path this_cpu() — we only set context on tasks we're about to switch to.
pub fn set_task_context(pid: Pid, context: crate::task::TaskContext) {
    let cpu = this_cpu();
    let found = with_rq(cpu, |rq| {
        if let Some(task) = rq.get_task_mut(pid) {
            task.context = context;
            true
        } else {
            false
        }
    });
    if found == Some(true) {
        return;
    }
    for other in 0..num_cpus() {
        if other == cpu {
            continue;
        }
        let found = with_rq(other, |rq| {
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

/// Save kernel_preempt_ok flag to a task's saved state.
///
/// — GraveShift: Called on switch-out. The per-CPU flag is about to be cleared,
/// so we stash the value in the task struct. On switch-in, load_kernel_preempt
/// restores it → no more lost preemption allowance → no more deadlock.
pub fn save_kernel_preempt(pid: Pid, value: bool) {
    // — SableWire: ISR-safe — use try_with_rq. Called from timer ISR during context
    // switch. If the lock can't be acquired (shouldn't happen since we checked
    // rq_lock_available earlier), silently skip — the task keeps its old value.
    let cpu = this_cpu();
    let found = try_with_rq(cpu, |rq| {
        if let Some(task) = rq.get_task_mut(pid) {
            task.kernel_preempt_ok = value;
            true
        } else {
            false
        }
    });
    if found == Some(true) {
        return;
    }
    for other in 0..num_cpus() {
        if other == cpu { continue; }
        let found = try_with_rq(other, |rq| {
            if let Some(task) = rq.get_task_mut(pid) {
                task.kernel_preempt_ok = value;
                true
            } else {
                false
            }
        });
        if found == Some(true) {
            return;
        }
    }
}

/// Load kernel_preempt_ok flag from a task's saved state.
///
/// — GraveShift: Called on switch-in. Returns the value saved at last switch-out.
/// ISR-safe — uses try_with_rq to avoid deadlock if lock is held.
pub fn load_kernel_preempt(pid: Pid) -> bool {
    let cpu = this_cpu();
    if let Some(val) = try_with_rq(cpu, |rq| rq.get_task(pid).map(|t| t.kernel_preempt_ok)).flatten() {
        return val;
    }
    for other in 0..num_cpus() {
        if other == cpu { continue; }
        if let Some(val) = try_with_rq(other, |rq| rq.get_task(pid).map(|t| t.kernel_preempt_ok)).flatten() {
            return val;
        }
    }
    false
}

/// Get task PML4 physical address (for address space switching)
///
/// — GraveShift: Fast-path this_cpu() — address space switches are always local.
pub fn get_task_pml4(pid: Pid) -> Option<PhysAddr> {
    let cpu = this_cpu();
    if let Some(pml4) = with_rq(cpu, |rq| rq.get_task(pid).map(|t| t.pml4_phys)).flatten() {
        return Some(pml4);
    }
    for other in 0..num_cpus() {
        if other == cpu {
            continue;
        }
        if let Some(pml4) = with_rq(other, |rq| rq.get_task(pid).map(|t| t.pml4_phys)).flatten() {
            return Some(pml4);
        }
    }
    None
}

/// Get task kernel stack info (for context switching)
///
/// — GraveShift: Fast-path this_cpu() — kernel stack queries are local-task ops.
pub fn get_task_kernel_stack(pid: Pid) -> Option<(PhysAddr, usize)> {
    let cpu = this_cpu();
    if let Some(info) = with_rq(cpu, |rq| {
        rq.get_task(pid)
            .map(|t| (t.kernel_stack, t.kernel_stack_size))
    })
    .flatten()
    {
        return Some(info);
    }
    for other in 0..num_cpus() {
        if other == cpu {
            continue;
        }
        if let Some(info) = with_rq(other, |rq| {
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
///
/// — GraveShift: Fast-path this_cpu() — exec always modifies the caller's own task.
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

    let cpu = this_cpu();
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
        return;
    }
    for other in 0..num_cpus() {
        if other == cpu {
            continue;
        }
        let updated = with_rq(other, |rq| {
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
///
/// — GraveShift: Fast-path this_cpu() — context switch info is always for local tasks.
pub fn get_task_switch_info(
    pid: Pid,
) -> Option<(crate::task::TaskContext, PhysAddr, PhysAddr, usize)> {
    let cpu = this_cpu();
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
    for other in 0..num_cpus() {
        if other == cpu {
            continue;
        }
        if let Some(info) = with_rq(other, |rq| {
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
///
/// — GraveShift: Fast-path this_cpu() for the lookup, then handle migration if needed.
pub fn set_affinity(pid: Pid, cpuset: CpuSet) {
    let start_cpu = this_cpu();
    let mut found_cpu = None;
    // Fast-path: try current CPU first
    let found = with_rq(start_cpu, |rq| {
        if let Some(task) = rq.get_task_mut(pid) {
            task.cpu_affinity = cpuset;
            true
        } else {
            false
        }
    });
    if found == Some(true) {
        found_cpu = Some(start_cpu);
    } else {
        for cpu in 0..num_cpus() {
            if cpu == start_cpu {
                continue;
            }
            let found = with_rq(cpu, |rq| {
                if let Some(task) = rq.get_task_mut(pid) {
                    task.cpu_affinity = cpuset;
                    true
                } else {
                    false
                }
            });
            if found == Some(true) {
                found_cpu = Some(cpu);
                break;
            }
        }
    }
    if let Some(cpu) = found_cpu {
        // ThreadRogue: Eager affinity enforcement - migrate immediately if disallowed
        if !cpuset.is_set(cpu) {
            // Task exists on this CPU but new affinity excludes this CPU
            // Extract the task if it's queued (not running)
            let maybe_task = with_rq(cpu, |rq| {
                if rq.curr() == Some(pid) {
                    // Current task: force reschedule so next schedule() will migrate
                    rq.set_need_resched(true);
                    if cpu != this_cpu() {
                        smp::ipi::send_reschedule(cpu);
                    }
                    None
                } else {
                    // Queued task: dequeue and extract for migration
                    rq.dequeue_task(pid);
                    rq.remove_task(pid)
                }
            });

            // Re-add task on allowed CPU (outside with_rq lock)
            if let Some(task) = maybe_task.flatten() {
                add_task(task);
            }
        }
    }
}

/// Get task CPU affinity
///
/// — GraveShift: Fast-path this_cpu().
pub fn get_affinity(pid: Pid) -> Option<CpuSet> {
    let cpu = this_cpu();
    if let Some(affinity) = with_rq(cpu, |rq| rq.get_task(pid).map(|t| t.cpu_affinity)).flatten() {
        return Some(affinity);
    }
    for other in 0..num_cpus() {
        if other == cpu {
            continue;
        }
        if let Some(affinity) =
            with_rq(other, |rq| rq.get_task(pid).map(|t| t.cpu_affinity)).flatten()
        {
            return Some(affinity);
        }
    }
    None
}

/// Set task scheduling policy
///
/// — GraveShift: Fast-path this_cpu().
pub fn set_scheduler(pid: Pid, policy: SchedPolicy, priority: u8) {
    let start_cpu = this_cpu();
    let do_set = |rq: &mut RunQueue| -> bool {
        let was_on_rq = rq.get_task(pid).map(|t| t.on_rq).unwrap_or(false);
        if rq.get_task(pid).is_none() {
            return false;
        }
        if was_on_rq {
            rq.dequeue_task(pid);
        }
        if let Some(task) = rq.get_task_mut(pid) {
            task.set_policy(policy);
            if policy.is_realtime() {
                task.set_rt_priority(priority);
            }
        }
        if was_on_rq {
            rq.enqueue_task(pid);
        }
        true
    };
    if with_rq(start_cpu, do_set) == Some(true) {
        return;
    }
    for other in 0..num_cpus() {
        if other == start_cpu {
            continue;
        }
        if with_rq(other, |rq| {
            let was_on_rq = rq.get_task(pid).map(|t| t.on_rq).unwrap_or(false);
            if rq.get_task(pid).is_none() {
                return false;
            }
            if was_on_rq {
                rq.dequeue_task(pid);
            }
            if let Some(task) = rq.get_task_mut(pid) {
                task.set_policy(policy);
                if policy.is_realtime() {
                    task.set_rt_priority(priority);
                }
            }
            if was_on_rq {
                rq.enqueue_task(pid);
            }
            true
        }) == Some(true)
        {
            break;
        }
    }
}

/// Get task scheduling policy
///
/// — GraveShift: Fast-path this_cpu().
pub fn get_scheduler(pid: Pid) -> Option<(SchedPolicy, u8)> {
    let cpu = this_cpu();
    if let Some(result) = with_rq(cpu, |rq| {
        rq.get_task(pid).map(|t| (t.policy, t.rt_priority))
    })
    .flatten()
    {
        return Some(result);
    }
    for other in 0..num_cpus() {
        if other == cpu {
            continue;
        }
        if let Some(result) = with_rq(other, |rq| {
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
///
/// — GraveShift: Fast-path this_cpu().
pub fn set_nice(pid: Pid, nice: i8) {
    let cpu = this_cpu();
    let found = with_rq(cpu, |rq| {
        if let Some(task) = rq.get_task_mut(pid) {
            task.set_nice(nice);
            true
        } else {
            false
        }
    });
    if found == Some(true) {
        return;
    }
    for other in 0..num_cpus() {
        if other == cpu {
            continue;
        }
        let found = with_rq(other, |rq| {
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
///
/// — GraveShift: Fast-path this_cpu().
pub fn get_nice(pid: Pid) -> Option<i8> {
    let cpu = this_cpu();
    if let Some(nice) = with_rq(cpu, |rq| rq.get_task(pid).map(|t| t.nice)).flatten() {
        return Some(nice);
    }
    for other in 0..num_cpus() {
        if other == cpu {
            continue;
        }
        if let Some(nice) = with_rq(other, |rq| rq.get_task(pid).map(|t| t.nice)).flatten() {
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

/// Debug info for a single task
#[derive(Clone)]
pub struct TaskDebugInfo {
    pub pid: Pid,
    pub ppid: Pid,
    pub state: TaskState,
    pub policy: SchedPolicy,
    pub on_rq: bool,
    pub nice: i8,
    pub weight: u64,
    pub vruntime: u64,
    pub sum_exec_runtime: u64,
    pub waiting_for_child: i32,
    /// First 16 bytes of cmdline (truncated for interrupt-safe printing)
    pub name: [u8; 16],
    pub name_len: usize,
}

/// Debug: full scheduler dump (interrupt-safe via try_lock)
///
/// Returns (current_pid, min_vruntime, tasks_info) or None if lock is held.
pub fn debug_dump_all() -> Option<(Option<Pid>, u64, Vec<TaskDebugInfo>)> {
    let cpu = this_cpu();
    try_with_rq(cpu, |rq| {
        let curr = rq.curr();
        let min_vr = rq.min_vruntime();
        let pids = rq.all_pids();
        let mut tasks = Vec::with_capacity(pids.len());
        for pid in pids {
            if let Some(t) = rq.get_task(pid) {
                let mut name = [0u8; 16];
                let mut name_len = 0;
                // Extract command name from ProcessMeta if available
                if let Some(ref meta_arc) = t.meta {
                    if let Some(meta) = meta_arc.try_lock() {
                        if let Some(cmd) = meta.cmdline.first() {
                            // Get the basename (after last '/')
                            let bytes = cmd.as_bytes();
                            let start = bytes
                                .iter()
                                .rposition(|&b| b == b'/')
                                .map(|i| i + 1)
                                .unwrap_or(0);
                            let len = (bytes.len() - start).min(16);
                            name[..len].copy_from_slice(&bytes[start..start + len]);
                            name_len = len;
                        }
                    }
                }
                if name_len == 0 && pid == 0 {
                    name[..4].copy_from_slice(b"idle");
                    name_len = 4;
                }
                tasks.push(TaskDebugInfo {
                    pid: t.pid,
                    ppid: t.ppid,
                    state: t.state,
                    policy: t.policy,
                    on_rq: t.on_rq,
                    nice: t.nice,
                    weight: t.weight,
                    vruntime: t.vruntime,
                    sum_exec_runtime: t.sum_exec_runtime,
                    waiting_for_child: t.waiting_for_child,
                    name,
                    name_len,
                });
            }
        }
        (curr, min_vr, tasks)
    })
}

// ============================================================================
// ProcessMeta accessor functions
// These functions provide access to process metadata through the scheduler
// ============================================================================

use alloc::sync::Arc;
use proc::ProcessMeta;

/// Get all PIDs across all CPUs
///
/// — GraveShift: Uses try_with_rq to avoid blocking on contended RQ locks.
/// Non-critical query — missing a PID because of lock contention is harmless
/// (procfs will just retry on next readdir).
pub fn all_pids() -> Vec<Pid> {
    let mut pids = Vec::new();
    for cpu in 0..num_cpus() {
        if let Some(mut cpu_pids) = try_with_rq(cpu, |rq| rq.all_pids()) {
            pids.append(&mut cpu_pids);
        }
    }
    pids
}

/// Get process metadata for a task by PID
///
/// — GraveShift: Fast-path tries this_cpu() first. With all user tasks on CPU 0,
/// this avoids the O(num_cpus) blocking lock loop on every syscall that touches meta.
pub fn get_task_meta(pid: Pid) -> Option<Arc<Mutex<ProcessMeta>>> {
    let cpu = this_cpu();
    if let Some(meta) = with_rq(cpu, |rq| rq.get_task(pid).and_then(|t| t.meta.clone())).flatten() {
        return Some(meta);
    }
    for other in 0..num_cpus() {
        if other == cpu {
            continue;
        }
        if let Some(meta) =
            with_rq(other, |rq| rq.get_task(pid).and_then(|t| t.meta.clone())).flatten()
        {
            return Some(meta);
        }
    }
    None
}

/// Get process metadata for the current task
///
/// — GraveShift: Ultra-fast path. The current task is ALWAYS on this_cpu()'s RQ.
/// No need to call current_pid() → get_task_meta() → loop all CPUs.
/// Single lock acquire, single RQ lookup.
pub fn get_current_meta() -> Option<Arc<Mutex<ProcessMeta>>> {
    let cpu = this_cpu();
    let result = with_rq(cpu, |rq| {
        let pid = rq.curr()?;
        rq.get_task(pid).and_then(|t| t.meta.clone())
    })
    .flatten();
    if result.is_some() {
        return result;
    }
    // — GraveShift: Fallback shouldn't happen, but don't panic if it does.
    current_pid().and_then(get_task_meta)
}

/// DEBUG: Module-level statics for with_current_meta diagnostics
pub static DEBUG_SCHED_META_PTR: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);
pub static DEBUG_SCHED_ARC_PTR: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// Execute a closure with read access to current task's ProcessMeta
pub fn with_current_meta<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&ProcessMeta) -> R,
{
    let meta_arc = get_current_meta()?;
    // Keep the guard alive for the entire closure execution
    let guard = meta_arc.lock();
    // Dereference the guard to get &ProcessMeta
    let meta_ref: &ProcessMeta = &*guard;

    // DEBUG: Log the addresses for investigation
    static DEBUG_CALLED: core::sync::atomic::AtomicBool =
        core::sync::atomic::AtomicBool::new(false);
    if DEBUG_CALLED.swap(true, core::sync::atomic::Ordering::SeqCst) == false {
        let meta_ptr = meta_ref as *const ProcessMeta as u64;
        let meta_arc_ptr = &meta_arc as *const _ as u64;
        DEBUG_SCHED_META_PTR.store(meta_ptr, core::sync::atomic::Ordering::SeqCst);
        DEBUG_SCHED_ARC_PTR.store(meta_arc_ptr, core::sync::atomic::Ordering::SeqCst);
    }

    Some(f(meta_ref))
}

/// Execute a closure with write access to current task's ProcessMeta
pub fn with_current_meta_mut<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut ProcessMeta) -> R,
{
    let meta_arc = get_current_meta()?;
    // Keep the guard alive for the entire closure execution
    let mut guard = meta_arc.lock();
    // Dereference the guard to get &mut ProcessMeta
    let meta_ref: &mut ProcessMeta = &mut *guard;
    Some(f(meta_ref))
}

/// Get the children of a task
///
/// — GraveShift: Fast-path this_cpu() — parent is almost always the caller.
pub fn get_task_children(pid: Pid) -> Vec<Pid> {
    let cpu = this_cpu();
    if let Some(children) =
        with_rq(cpu, |rq| rq.get_task(pid).map(|t| t.children.clone())).flatten()
    {
        return children;
    }
    for other in 0..num_cpus() {
        if other == cpu {
            continue;
        }
        if let Some(children) =
            with_rq(other, |rq| rq.get_task(pid).map(|t| t.children.clone())).flatten()
        {
            return children;
        }
    }
    Vec::new()
}

/// Add a child to a task
///
/// — GraveShift: Fast-path this_cpu() — parent adding child is always local.
pub fn add_task_child(pid: Pid, child_pid: Pid) {
    let cpu = this_cpu();
    let found = with_rq(cpu, |rq| {
        if let Some(task) = rq.get_task_mut(pid) {
            task.add_child(child_pid);
            true
        } else {
            false
        }
    });
    if found == Some(true) {
        return;
    }
    for other in 0..num_cpus() {
        if other == cpu {
            continue;
        }
        let found = with_rq(other, |rq| {
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
///
/// — GraveShift: Fast-path this_cpu().
pub fn remove_task_child(pid: Pid, child_pid: Pid) {
    let cpu = this_cpu();
    let found = with_rq(cpu, |rq| {
        if let Some(task) = rq.get_task_mut(pid) {
            task.remove_child(child_pid);
            true
        } else {
            false
        }
    });
    if found == Some(true) {
        return;
    }
    for other in 0..num_cpus() {
        if other == cpu {
            continue;
        }
        let found = with_rq(other, |rq| {
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
///
/// — GraveShift: Fast-path this_cpu() — exit() is called by the dying task itself.
pub fn set_task_exit_status(pid: Pid, status: i32) {
    let cpu = this_cpu();
    let found = with_rq(cpu, |rq| {
        if let Some(task) = rq.get_task_mut(pid) {
            task.exit(status);
            rq.dequeue_task(pid);
            true
        } else {
            false
        }
    });
    if found == Some(true) {
        return;
    }
    for other in 0..num_cpus() {
        if other == cpu {
            continue;
        }
        let found = with_rq(other, |rq| {
            if let Some(task) = rq.get_task_mut(pid) {
                task.exit(status);
                rq.dequeue_task(pid);
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
///
/// — GraveShift: Fast-path this_cpu() — waitpid calls this on children, usually local.
pub fn get_task_ppid(pid: Pid) -> Option<Pid> {
    let cpu = this_cpu();
    if let Some(ppid) = with_rq(cpu, |rq| rq.get_task(pid).map(|t| t.ppid)).flatten() {
        return Some(ppid);
    }
    for other in 0..num_cpus() {
        if other == cpu {
            continue;
        }
        if let Some(ppid) = with_rq(other, |rq| rq.get_task(pid).map(|t| t.ppid)).flatten() {
            return Some(ppid);
        }
    }
    None
}

/// Get task timing info for /proc/[pid]/stat
/// Returns (state, ppid, start_time, sum_exec_runtime, nice)
///
/// — GraveShift: Fast-path this_cpu(). Procfs reads /proc/[pid]/stat for every process
/// in `top` — this cuts lock contention from O(N*CPUs) to O(N).
pub fn get_task_timing_info(pid: Pid) -> Option<(TaskState, Pid, u64, u64, i8)> {
    let cpu = this_cpu();
    if let Some(info) = with_rq(cpu, |rq| {
        rq.get_task(pid)
            .map(|t| (t.state, t.ppid, t.start_time, t.sum_exec_runtime, t.nice))
    })
    .flatten()
    {
        return Some(info);
    }
    for other in 0..num_cpus() {
        if other == cpu {
            continue;
        }
        if let Some(info) = with_rq(other, |rq| {
            rq.get_task(pid)
                .map(|t| (t.state, t.ppid, t.start_time, t.sum_exec_runtime, t.nice))
        })
        .flatten()
        {
            return Some(info);
        }
    }
    None
}

/// Get exit status of a task (if zombie)
///
/// — GraveShift: Fast-path this_cpu() — zombies are on the CPU they died on.
pub fn get_task_exit_status(pid: Pid) -> Option<i32> {
    let cpu = this_cpu();
    if let Some((state, status)) =
        with_rq(cpu, |rq| rq.get_task(pid).map(|t| (t.state, t.exit_status))).flatten()
    {
        if state == TaskState::TASK_ZOMBIE {
            return Some(status);
        }
        return None;
    }
    for other in 0..num_cpus() {
        if other == cpu {
            continue;
        }
        if let Some((state, status)) = with_rq(other, |rq| {
            rq.get_task(pid).map(|t| (t.state, t.exit_status))
        })
        .flatten()
        {
            if state == TaskState::TASK_ZOMBIE {
                return Some(status);
            }
            return None;
        }
    }
    None
}

/// Check if a task is waiting for a specific child
///
/// — GraveShift: Fast-path this_cpu() — waitpid checks are always local.
pub fn is_task_waiting_for(pid: Pid, child_pid: Pid) -> bool {
    let cpu = this_cpu();
    if let Some(waiting) = with_rq(cpu, |rq| {
        rq.get_task(pid).map(|t| t.is_waiting_for(child_pid))
    })
    .flatten()
    {
        return waiting;
    }
    for other in 0..num_cpus() {
        if other == cpu {
            continue;
        }
        if let Some(waiting) = with_rq(other, |rq| {
            rq.get_task(pid).map(|t| t.is_waiting_for(child_pid))
        })
        .flatten()
        {
            return waiting;
        }
    }
    false
}

/// Set a task to wait for a child
///
/// — GraveShift: Fast-path this_cpu() — waitpid sets waiting on the caller.
pub fn set_task_waiting(pid: Pid, child_pid: i32) {
    let cpu = this_cpu();
    let found = with_rq(cpu, |rq| {
        if let Some(task) = rq.get_task_mut(pid) {
            task.wait_for_child(child_pid);
            true
        } else {
            false
        }
    });
    if found == Some(true) {
        return;
    }
    for other in 0..num_cpus() {
        if other == cpu {
            continue;
        }
        let found = with_rq(other, |rq| {
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
///
/// — GraveShift: Fast-path this_cpu().
pub fn clear_task_waiting(pid: Pid) {
    let cpu = this_cpu();
    let found = with_rq(cpu, |rq| {
        if let Some(task) = rq.get_task_mut(pid) {
            task.clear_waiting();
            true
        } else {
            false
        }
    });
    if found == Some(true) {
        return;
    }
    for other in 0..num_cpus() {
        if other == cpu {
            continue;
        }
        let found = with_rq(other, |rq| {
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
///
/// — GraveShift: Fast-path this_cpu().
pub fn set_task_meta(pid: Pid, meta: Arc<Mutex<ProcessMeta>>) {
    let cpu = this_cpu();
    let found = with_rq(cpu, |rq| {
        if let Some(task) = rq.get_task_mut(pid) {
            task.set_meta(meta.clone());
            true
        } else {
            false
        }
    });
    if found == Some(true) {
        return;
    }
    for other in 0..num_cpus() {
        if other == cpu {
            continue;
        }
        let found = with_rq(other, |rq| {
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

/// Get nice value for a task
///
/// — GraveShift: Fast-path this_cpu().
pub fn get_task_nice(pid: Pid) -> Option<i8> {
    let cpu = this_cpu();
    if let Some(nice) = with_rq(cpu, |rq| rq.get_task(pid).map(|t| t.nice)).flatten() {
        return Some(nice);
    }
    for other in 0..num_cpus() {
        if other == cpu {
            continue;
        }
        if let Some(nice) = with_rq(other, |rq| rq.get_task(pid).map(|t| t.nice)).flatten() {
            return Some(nice);
        }
    }
    None
}

/// Set nice value for a task
///
/// — GraveShift: Fast-path this_cpu().
pub fn set_task_nice(pid: Pid, nice: i8) -> bool {
    let cpu = this_cpu();
    if with_rq(cpu, |rq| {
        if let Some(task) = rq.get_task_mut(pid) {
            task.set_nice(nice);
            true
        } else {
            false
        }
    }) == Some(true)
    {
        return true;
    }
    for other in 0..num_cpus() {
        if other == cpu {
            continue;
        }
        if with_rq(other, |rq| {
            if let Some(task) = rq.get_task_mut(pid) {
                task.set_nice(nice);
                true
            } else {
                false
            }
        }) == Some(true)
        {
            return true;
        }
    }
    false
}

/// Get scheduler policy for a task
///
/// — GraveShift: Fast-path this_cpu().
pub fn get_task_policy(pid: Pid) -> Option<SchedPolicy> {
    let cpu = this_cpu();
    if let Some(policy) = with_rq(cpu, |rq| rq.get_task(pid).map(|t| t.policy)).flatten() {
        return Some(policy);
    }
    for other in 0..num_cpus() {
        if other == cpu {
            continue;
        }
        if let Some(policy) = with_rq(other, |rq| rq.get_task(pid).map(|t| t.policy)).flatten() {
            return Some(policy);
        }
    }
    None
}

/// Set scheduler policy for a task
///
/// — GraveShift: Fast-path this_cpu().
pub fn set_task_policy(pid: Pid, policy: SchedPolicy) -> bool {
    let cpu = this_cpu();
    if with_rq(cpu, |rq| {
        if let Some(task) = rq.get_task_mut(pid) {
            task.policy = policy;
            true
        } else {
            false
        }
    }) == Some(true)
    {
        return true;
    }
    for other in 0..num_cpus() {
        if other == cpu {
            continue;
        }
        if with_rq(other, |rq| {
            if let Some(task) = rq.get_task_mut(pid) {
                task.policy = policy;
                true
            } else {
                false
            }
        }) == Some(true)
        {
            return true;
        }
    }
    false
}

/// Get RT priority for a task
///
/// — GraveShift: Fast-path this_cpu().
pub fn get_task_rt_priority(pid: Pid) -> Option<u8> {
    let cpu = this_cpu();
    if let Some(prio) = with_rq(cpu, |rq| rq.get_task(pid).map(|t| t.rt_priority)).flatten() {
        return Some(prio);
    }
    for other in 0..num_cpus() {
        if other == cpu {
            continue;
        }
        if let Some(prio) = with_rq(other, |rq| rq.get_task(pid).map(|t| t.rt_priority)).flatten() {
            return Some(prio);
        }
    }
    None
}

/// Set RT priority for a task
///
/// — GraveShift: Fast-path this_cpu().
pub fn set_task_rt_priority(pid: Pid, priority: u8) -> bool {
    let cpu = this_cpu();
    if with_rq(cpu, |rq| {
        if let Some(task) = rq.get_task_mut(pid) {
            task.rt_priority = priority;
            true
        } else {
            false
        }
    }) == Some(true)
    {
        return true;
    }
    for other in 0..num_cpus() {
        if other == cpu {
            continue;
        }
        if with_rq(other, |rq| {
            if let Some(task) = rq.get_task_mut(pid) {
                task.rt_priority = priority;
                true
            } else {
                false
            }
        }) == Some(true)
        {
            return true;
        }
    }
    false
}

/// Get CPU affinity for a task
///
/// — GraveShift: Fast-path this_cpu().
pub fn get_task_affinity(pid: Pid) -> Option<CpuSet> {
    let cpu = this_cpu();
    if let Some(affinity) =
        with_rq(cpu, |rq| rq.get_task(pid).map(|t| t.cpu_affinity.clone())).flatten()
    {
        return Some(affinity);
    }
    for other in 0..num_cpus() {
        if other == cpu {
            continue;
        }
        if let Some(affinity) =
            with_rq(other, |rq| rq.get_task(pid).map(|t| t.cpu_affinity.clone())).flatten()
        {
            return Some(affinity);
        }
    }
    None
}

/// Set CPU affinity for a task
///
/// — GraveShift: Fast-path this_cpu().
pub fn set_task_affinity(pid: Pid, affinity: CpuSet) -> bool {
    let cpu = this_cpu();
    if with_rq(cpu, |rq| {
        if let Some(task) = rq.get_task_mut(pid) {
            task.cpu_affinity = affinity.clone();
            true
        } else {
            false
        }
    }) == Some(true)
    {
        return true;
    }
    for other in 0..num_cpus() {
        if other == cpu {
            continue;
        }
        if with_rq(other, |rq| {
            if let Some(task) = rq.get_task_mut(pid) {
                task.cpu_affinity = affinity.clone();
                true
            } else {
                false
            }
        }) == Some(true)
        {
            return true;
        }
    }
    false
}
