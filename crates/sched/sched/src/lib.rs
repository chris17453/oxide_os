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
    CpuSet, Pid, SchedPolicy, TaskState,
    Context, ContextSwitch, SchedClass, RunQueueOps,
    nice_to_weight, NICE_0_WEIGHT, NICE_TO_WEIGHT,
    RT_PRIO_MIN, RT_PRIO_MAX, RR_TIME_SLICE_NS, TICK_NS,
};

// Re-export core functions for convenience
pub use crate::core::{
    // Initialization
    init_cpu, this_cpu, set_this_cpu, num_cpus,
    // Task management
    add_task, remove_task, create_task,
    // Scheduling
    scheduler_tick, pick_next_task, need_resched, set_need_resched,
    // Task state
    current_pid, get_task_state,
    wake_up, block_current, yield_current,
    // Policy and priority
    set_scheduler, get_scheduler, set_nice, get_nice,
    // Affinity
    set_affinity, get_affinity,
    // Groups
    set_task_group, get_task_group,
    // Preemption control
    preempt_disable, preempt_enable, preempt_disabled,
    // Clock
    global_clock, update_clock,
};

// Re-export task types
pub use crate::task::{Task, TaskContext};

// Re-export run queue type
pub use crate::runqueue::RunQueue;

// Re-export scheduling classes
pub use crate::rt::{RtRunQueue, RtSchedClass};
pub use crate::fair::{CfsRunQueue, FairSchedClass};
pub use crate::idle::{IdleSchedClass, IdleTask};

// Re-export group types
pub use crate::group::{GroupId, SchedGroup, SchedGroups, group_id};
