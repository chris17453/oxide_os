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

// Re-export core functions for convenience
pub use crate::core::{
    // Task management
    add_task,
    block_current,
    create_task,
    create_task_with_meta,
    // Task state
    current_pid,
    // Debug
    debug_state,
    get_affinity,
    get_nice,
    get_scheduler,
    // Context switching support
    get_task_context,
    get_task_group,
    get_task_kernel_stack,
    get_task_pml4,
    get_task_state,
    get_task_switch_info,
    // Clock
    global_clock,
    // Initialization
    init_cpu,
    need_resched,
    num_cpus,
    pick_next_task,
    // Preemption control
    preempt_disable,
    preempt_disabled,
    preempt_enable,
    remove_task,
    // Scheduling
    scheduler_tick,
    // Affinity
    set_affinity,
    set_need_resched,
    set_nice,
    // Policy and priority
    set_scheduler,
    set_task_context,
    // Groups
    set_task_group,
    set_this_cpu,
    // Manual context switch support (for fork/exec)
    switch_to,
    this_cpu,
    update_clock,
    update_task_exec_info,
    wake_up,
    yield_current,
    // ProcessMeta accessors
    get_task_meta,
    get_current_meta,
    with_current_meta,
    with_current_meta_mut,
    get_task_children,
    add_task_child,
    remove_task_child,
    set_task_exit_status,
    get_task_ppid,
    get_task_exit_status,
    is_task_waiting_for,
    set_task_waiting,
    clear_task_waiting,
    set_task_meta,
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
