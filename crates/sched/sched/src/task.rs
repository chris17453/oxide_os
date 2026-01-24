//! Task structure for the scheduler
//!
//! This module defines the Task structure which is the primary schedulable
//! entity in OXIDE. Like Linux's task_struct, it contains both scheduling
//! state and execution context.
//!
//! Process metadata (fd_table, signals, credentials) is stored separately
//! in ProcessMeta and looked up by PID when syscalls need it.

extern crate alloc;

use alloc::vec::Vec;
use os_core::PhysAddr;
use sched_traits::{CpuSet, Pid, SchedPolicy, TaskState, nice_to_weight, NICE_0_WEIGHT};

/// Saved CPU context for context switching
///
/// Contains all registers that need to be saved/restored when switching tasks.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct TaskContext {
    /// Instruction pointer
    pub rip: u64,
    /// Stack pointer
    pub rsp: u64,
    /// Flags register
    pub rflags: u64,
    /// General purpose registers
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    /// Code segment (0x23 for user, 0x08 for kernel)
    pub cs: u64,
    /// Stack segment (0x1B for user, 0x10 for kernel)
    pub ss: u64,
}

/// The primary schedulable entity in OXIDE
///
/// Like Linux's task_struct, this contains both scheduling state and
/// execution context. This is THE authority on task state - there is
/// no duplicate state elsewhere.
#[derive(Clone)]
pub struct Task {
    /// Process/thread identifier
    pub pid: Pid,

    /// Parent task ID (0 for init)
    pub ppid: Pid,

    /// Current task state - THIS IS THE AUTHORITY
    pub state: TaskState,

    /// Scheduling policy (Normal, FIFO, RR, Batch, Idle)
    pub policy: SchedPolicy,

    // ========================================
    // Execution context
    // ========================================
    /// Saved CPU registers for context switching
    pub context: TaskContext,

    /// Physical address of PML4 (page table root) for address space switching
    pub pml4_phys: PhysAddr,

    /// User stack top address
    pub user_stack_top: u64,

    /// Entry point address
    pub entry_point: u64,

    // ========================================
    // Real-time scheduling fields
    // ========================================
    /// RT priority (1-99, higher = more priority)
    /// Only meaningful for FIFO and RR policies
    pub rt_priority: u8,

    /// Time slice remaining (in ticks) for RR policy
    pub time_slice: u32,

    // ========================================
    // CFS (fair) scheduling fields
    // ========================================
    /// Nice value (-20 to +19, lower = higher priority)
    pub nice: i8,

    /// Weight derived from nice value
    /// Higher weight = more CPU time
    pub weight: u64,

    /// Virtual runtime in nanoseconds
    /// Tasks with lower vruntime get scheduled first
    pub vruntime: u64,

    // ========================================
    // CPU affinity
    // ========================================
    /// Set of CPUs this task is allowed to run on
    pub cpu_affinity: CpuSet,

    /// Last CPU this task ran on (for cache affinity)
    pub last_cpu: u32,

    // ========================================
    // Accounting
    // ========================================
    /// Timestamp when task started running (nanoseconds)
    pub exec_start: u64,

    /// Total runtime accumulated (nanoseconds)
    pub sum_exec_runtime: u64,

    // ========================================
    // Preemption control
    // ========================================
    /// Preemption disable counter
    /// When > 0, this task cannot be preempted
    pub preempt_count: u32,

    /// Flag indicating reschedule is needed
    pub need_resched: bool,

    // ========================================
    // Kernel stack
    // ========================================
    /// Physical address of kernel stack
    pub kernel_stack: PhysAddr,

    /// Size of kernel stack in bytes
    pub kernel_stack_size: usize,

    // ========================================
    // Wake queue linkage
    // ========================================
    /// True if task is on a run queue
    pub on_rq: bool,

    // ========================================
    // Process relationships
    // ========================================
    /// Child task PIDs
    pub children: alloc::vec::Vec<Pid>,

    /// Exit status (valid when state is TASK_ZOMBIE)
    pub exit_status: i32,

    /// PID of child being waited for (0 = not waiting, -1 = any child)
    pub waiting_for_child: i32,
}

impl Task {
    /// Create a new task with default scheduling parameters
    pub fn new(
        pid: Pid,
        ppid: Pid,
        kernel_stack: PhysAddr,
        kernel_stack_size: usize,
        pml4_phys: PhysAddr,
        entry_point: u64,
        user_stack_top: u64,
    ) -> Self {
        Self {
            pid,
            ppid,
            state: TaskState::TASK_RUNNING,
            policy: SchedPolicy::Normal,
            context: TaskContext::default(),
            pml4_phys,
            user_stack_top,
            entry_point,
            rt_priority: 0,
            time_slice: 0,
            nice: 0,
            weight: NICE_0_WEIGHT,
            vruntime: 0,
            cpu_affinity: CpuSet::all(),
            last_cpu: 0,
            exec_start: 0,
            sum_exec_runtime: 0,
            preempt_count: 0,
            need_resched: false,
            kernel_stack,
            kernel_stack_size,
            on_rq: false,
            children: Vec::new(),
            exit_status: 0,
            waiting_for_child: 0,
        }
    }

    /// Create a new idle task for a specific CPU
    pub fn new_idle(pid: Pid, cpu: u32, kernel_stack: PhysAddr, kernel_stack_size: usize) -> Self {
        Self {
            pid,
            ppid: 0,
            state: TaskState::TASK_RUNNING,
            policy: SchedPolicy::Idle,
            context: TaskContext::default(),
            pml4_phys: PhysAddr::new(0), // Idle task uses kernel page tables
            user_stack_top: 0,
            entry_point: 0,
            rt_priority: 0,
            time_slice: 0,
            nice: 19, // Lowest priority
            weight: nice_to_weight(19),
            vruntime: 0,
            cpu_affinity: CpuSet::single(cpu), // Pinned to specific CPU
            last_cpu: cpu,
            exec_start: 0,
            sum_exec_runtime: 0,
            preempt_count: 0,
            need_resched: false,
            kernel_stack,
            kernel_stack_size,
            on_rq: false,
            children: Vec::new(),
            exit_status: 0,
            waiting_for_child: 0,
        }
    }

    /// Set the scheduling policy
    pub fn set_policy(&mut self, policy: SchedPolicy) {
        self.policy = policy;
        // Reset time slice for RR
        if policy == SchedPolicy::RoundRobin {
            self.time_slice = policy.default_time_slice();
        }
    }

    /// Set the nice value and update weight
    pub fn set_nice(&mut self, nice: i8) {
        self.nice = nice.clamp(-20, 19);
        self.weight = nice_to_weight(self.nice);
    }

    /// Set the RT priority
    pub fn set_rt_priority(&mut self, prio: u8) {
        self.rt_priority = prio.clamp(1, 99);
    }

    /// Check if this is a real-time task
    pub fn is_rt(&self) -> bool {
        self.policy.is_realtime()
    }

    /// Check if this is a fair (CFS) task
    pub fn is_fair(&self) -> bool {
        self.policy.is_fair()
    }

    /// Check if this is an idle task
    pub fn is_idle(&self) -> bool {
        self.policy == SchedPolicy::Idle
    }

    /// Check if task can run on the given CPU
    pub fn can_run_on(&self, cpu: u32) -> bool {
        self.cpu_affinity.is_set(cpu)
    }

    /// Disable preemption
    pub fn preempt_disable(&mut self) {
        self.preempt_count = self.preempt_count.saturating_add(1);
    }

    /// Enable preemption
    pub fn preempt_enable(&mut self) {
        self.preempt_count = self.preempt_count.saturating_sub(1);
    }

    /// Check if preemption is disabled
    pub fn preempt_disabled(&self) -> bool {
        self.preempt_count > 0
    }

    /// Mark that reschedule is needed
    pub fn set_need_resched(&mut self) {
        self.need_resched = true;
    }

    /// Clear reschedule flag
    pub fn clear_need_resched(&mut self) {
        self.need_resched = false;
    }

    /// Update accounting when task starts running
    pub fn account_start(&mut self, now: u64) {
        self.exec_start = now;
    }

    /// Update accounting when task stops running
    /// Returns the delta runtime
    pub fn account_stop(&mut self, now: u64) -> u64 {
        let delta = now.saturating_sub(self.exec_start);
        self.sum_exec_runtime = self.sum_exec_runtime.saturating_add(delta);
        self.exec_start = 0;
        delta
    }

    /// Update vruntime based on actual runtime
    ///
    /// vruntime = delta * (NICE_0_WEIGHT / task.weight)
    /// Lower weight tasks accumulate vruntime faster, so they get
    /// scheduled less often.
    pub fn update_vruntime(&mut self, delta: u64) {
        // Calculate weighted delta
        // Use saturating arithmetic to prevent overflow
        let weighted_delta = if self.weight > 0 {
            // vruntime = delta * (NICE_0_WEIGHT / weight)
            // To avoid division issues, compute as: delta * NICE_0_WEIGHT / weight
            let scaled = (delta as u128) * (NICE_0_WEIGHT as u128);
            (scaled / self.weight as u128) as u64
        } else {
            delta
        };
        self.vruntime = self.vruntime.saturating_add(weighted_delta);
    }

    // ========================================
    // Context access
    // ========================================

    /// Get a reference to the saved context
    pub fn context(&self) -> &TaskContext {
        &self.context
    }

    /// Get a mutable reference to the saved context
    pub fn context_mut(&mut self) -> &mut TaskContext {
        &mut self.context
    }

    /// Get the PML4 physical address
    pub fn pml4_phys(&self) -> PhysAddr {
        self.pml4_phys
    }

    /// Set the PML4 physical address (for exec)
    pub fn set_pml4_phys(&mut self, pml4: PhysAddr) {
        self.pml4_phys = pml4;
    }

    // ========================================
    // Child management
    // ========================================

    /// Add a child task
    pub fn add_child(&mut self, child_pid: Pid) {
        self.children.push(child_pid);
    }

    /// Remove a child task
    pub fn remove_child(&mut self, child_pid: Pid) {
        self.children.retain(|&pid| pid != child_pid);
    }

    /// Get child PIDs
    pub fn children(&self) -> &[Pid] {
        &self.children
    }

    /// Set which child to wait for (0 = not waiting, -1 = any)
    pub fn wait_for_child(&mut self, child_pid: i32) {
        self.waiting_for_child = child_pid;
        if child_pid != 0 {
            self.state = TaskState::TASK_INTERRUPTIBLE;
        }
    }

    /// Check if waiting for a specific child
    pub fn is_waiting_for(&self, child_pid: Pid) -> bool {
        match self.waiting_for_child {
            0 => false,
            -1 => true, // Waiting for any child
            pid => pid as Pid == child_pid,
        }
    }

    /// Clear waiting state
    pub fn clear_waiting(&mut self) {
        self.waiting_for_child = 0;
        self.state = TaskState::TASK_RUNNING;
    }

    /// Set exit status and transition to zombie
    pub fn exit(&mut self, status: i32) {
        self.exit_status = status;
        self.state = TaskState::TASK_ZOMBIE;
    }
}

impl core::fmt::Debug for Task {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Task")
            .field("pid", &self.pid)
            .field("state", &self.state)
            .field("policy", &self.policy)
            .field("rt_priority", &self.rt_priority)
            .field("nice", &self.nice)
            .field("vruntime", &self.vruntime)
            .field("on_rq", &self.on_rq)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_task(pid: Pid) -> Task {
        Task::new(
            pid,
            0, // ppid
            PhysAddr::new(0x1000),
            4096,
            PhysAddr::new(0x2000), // pml4
            0x400000, // entry
            0x7fff0000, // stack
        )
    }

    #[test]
    fn test_task_creation() {
        let task = test_task(1);
        assert_eq!(task.pid, 1);
        assert_eq!(task.state, TaskState::TASK_RUNNING);
        assert_eq!(task.policy, SchedPolicy::Normal);
        assert_eq!(task.nice, 0);
        assert_eq!(task.weight, NICE_0_WEIGHT);
    }

    #[test]
    fn test_nice_affects_weight() {
        let mut task = test_task(1);

        task.set_nice(-10);
        let high_weight = task.weight;

        task.set_nice(10);
        let low_weight = task.weight;

        assert!(high_weight > low_weight);
    }

    #[test]
    fn test_vruntime_update() {
        let mut task = test_task(1);
        task.set_nice(0); // weight = 1024

        // With nice=0, delta should equal vruntime increase
        task.update_vruntime(1000);
        assert_eq!(task.vruntime, 1000);

        // With higher nice (lower weight), vruntime grows faster
        task.set_nice(10);
        task.update_vruntime(1000);
        assert!(task.vruntime > 2000);
    }
}
