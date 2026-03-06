//! Linux-model scheduler implementation for OXIDE OS
//!
//! This scheduler implements a Linux-like scheduling model with:
//! - Multiple scheduling classes (RT, Fair/CFS, Idle)
//! - Per-CPU run queues
//! - vruntime-based fair scheduling with nice/weight
//! - CPU affinity support
//! - Scheduling groups for bandwidth control
//!
//! # Architecture
//!
//! ```text
//! +------------------+
//! |   schedule()     |  <-- Main entry point
//! +------------------+
//!          |
//!          v
//! +------------------+
//! |   pick_next()    |  <-- Iterates through classes
//! +------------------+
//!          |
//!     +----+----+----+
//!     |    |    |    |
//!     v    v    v    v
//!   [RT] [Fair] [Idle]  <-- Scheduling classes
//! ```
//!
//! # Scheduling Classes
//!
//! Classes are checked in priority order: RT > Fair > Idle
//!
//! - **RT (Real-time)**: SCHED_FIFO and SCHED_RR policies
//!   - Priority-based (1-99, higher = more important)
//!   - FIFO: runs until blocked or yielded
//!   - RR: time-sliced round robin
//!
//! - **Fair (CFS)**: SCHED_NORMAL and SCHED_BATCH policies
//!   - vruntime-based fairness
//!   - Nice values affect vruntime accumulation rate
//!   - Lower vruntime = scheduled first
//!
//! - **Idle**: SCHED_IDLE policy
//!   - Only runs when no other tasks available
//!   - Each CPU has one idle task
//!
//! # Example Usage
//!
//! ```ignore
//! use sched::{core, task::Task};
//!
//! // Initialize scheduler for CPU 0 with idle task PID 0
//! core::init_cpu(0, 0);
//!
//! // Create and add a task
//! let mut task = Task::new(1, kernel_stack, stack_size);
//! task.policy = SchedPolicy::Normal;
//! core::add_task(task);
//!
//! // In timer interrupt:
//! if core::scheduler_tick() {
//!     // Preemption needed
//!     let next = core::pick_next_task();
//!     // ... context switch ...
//! }
//! ```

#![no_std]

extern crate alloc;

pub mod core;
pub mod fair;
pub mod group;
pub mod idle;
pub mod rt;
pub mod runqueue;
pub mod task;

// Re-export commonly used types from sched-traits
pub use sched_traits::{
    Context, ContextSwitch, CpuSet, NICE_0_WEIGHT, NICE_TO_WEIGHT, Pid, RR_TIME_SLICE_NS,
    RT_PRIO_MAX, RT_PRIO_MIN, RunQueueOps, SchedClass, SchedPolicy, TICK_NS, TaskState,
    nice_to_weight,
};

// Re-export commonly used types from this crate
pub use crate::core::SwitchInfo;

// Re-export core functions for convenience
pub use crate::core::{
    TaskDebugInfo,
    // Task management
    add_task,
    add_task_to_cpu,
    // O(1) PID-to-CPU hint table (P3.9)
    pid_to_cpu,
    add_task_child,
    all_pids,
    block_current,
    clear_task_waiting,
    create_task,
    create_task_with_meta,
    // Task state
    current_pid,
    current_pid_lockfree,
    debug_dump_all,
    // Debug
    debug_state,
    try_debug_state_cpu,
    get_affinity,
    get_cpu_times,
    get_current_meta,
    get_nice,
    get_scheduler,
    get_task_affinity,
    get_task_children,
    // Atomic context-switch transaction (P2.1)
    context_switch_transaction,
    // Context switching support
    get_task_context,
    get_task_exit_status,
    get_task_group,
    get_task_kernel_stack,
    // SMP idle work stealing
    idle_try_steal,
    // ProcessMeta accessors
    get_task_meta,
    try_get_task_meta,
    // Task scheduling properties
    get_task_nice,
    get_task_pml4,
    get_task_policy,
    // Kernel preemption save/restore (per-task)
    save_kernel_preempt,
    load_kernel_preempt,
    // ISR lock safety
    rq_lock_available,
    get_task_ppid,
    try_get_task_ppid,
    get_task_rt_priority,
    get_task_state,
    try_get_task_state,
    get_task_switch_info,
    get_task_timing_info,
    // Clock
    global_clock,
    // Initialization
    init_cpu,
    is_task_waiting_for,
    need_resched,
    num_cpus,
    pick_next_task,
    // Preemption control
    preempt_disable,
    preempt_disabled,
    preempt_enable,
    // SMP CPU ID callback
    register_cpu_id_fn,
    remove_task,
    remove_task_child,
    // Scheduling
    scheduler_tick,
    scheduler_tick_ex,
    // Affinity
    set_affinity,
    set_need_resched,
    set_nice,
    // Policy and priority
    set_scheduler,
    set_task_affinity,
    set_task_context,
    set_task_exit_status,
    // Groups
    set_task_group,
    set_task_meta,
    set_task_nice,
    set_task_policy,
    set_task_rt_priority,
    set_task_waiting,
    set_this_cpu,
    // Manual context switch support (for fork/exec)
    switch_to,
    this_cpu,
    // ISR-safe wake (non-blocking, for timer interrupt)
    try_wake_up,
    update_clock,
    update_task_exec_info,
    wake_up,
    with_current_meta,
    with_current_meta_mut,
    yield_current,
};

// Re-export task types
pub use crate::task::{Task, TaskContext};

// Re-export ProcessMeta from proc crate
pub use proc::ProcessMeta;

// Re-export run queue type
pub use crate::runqueue::RunQueue;

// Re-export scheduling classes
pub use crate::fair::{CfsRunQueue, FairSchedClass};
pub use crate::idle::{IdleSchedClass, IdleTask};
pub use crate::rt::{RtRunQueue, RtSchedClass};

// Re-export group types
pub use crate::group::{GroupId, SchedGroup, SchedGroups, group_id};
