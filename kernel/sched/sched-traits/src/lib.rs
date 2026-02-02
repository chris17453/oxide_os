//! Scheduler trait definitions for OXIDE OS
//!
//! Provides architecture-independent interfaces for the Linux-model scheduler.

#![no_std]

/// Process/Thread identifier
pub type Pid = u32;

/// Task state following Linux model
///
/// Unlike simple Ready/Running/Blocked states, Linux uses a bitmask model
/// that allows for more nuanced states (e.g., interruptible vs uninterruptible sleep).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct TaskState(pub u32);

impl TaskState {
    /// Task is runnable (on a run queue, ready to be scheduled)
    pub const TASK_RUNNING: Self = Self(0);
    /// Task is sleeping, can be woken by signals
    pub const TASK_INTERRUPTIBLE: Self = Self(1);
    /// Task is sleeping, cannot be woken by signals (e.g., waiting for disk I/O)
    pub const TASK_UNINTERRUPTIBLE: Self = Self(2);
    /// Task is stopped (e.g., SIGSTOP received)
    pub const TASK_STOPPED: Self = Self(4);
    /// Task is being traced (ptrace)
    pub const TASK_TRACED: Self = Self(8);
    /// Task is exiting
    pub const TASK_DEAD: Self = Self(16);
    /// Task is a zombie (exited but not yet reaped)
    pub const TASK_ZOMBIE: Self = Self(32);

    /// Create a new task state
    pub const fn new(state: u32) -> Self {
        Self(state)
    }

    /// Get the raw state value
    pub const fn bits(&self) -> u32 {
        self.0
    }

    /// Check if task is runnable
    pub const fn is_runnable(&self) -> bool {
        self.0 == Self::TASK_RUNNING.0
    }

    /// Check if task is sleeping (interruptible or uninterruptible)
    pub const fn is_sleeping(&self) -> bool {
        (self.0 & (Self::TASK_INTERRUPTIBLE.0 | Self::TASK_UNINTERRUPTIBLE.0)) != 0
    }

    /// Check if task can be woken by a signal
    pub const fn is_interruptible(&self) -> bool {
        self.0 == Self::TASK_INTERRUPTIBLE.0
    }

    /// Check if task is dead or zombie
    pub const fn is_dead(&self) -> bool {
        (self.0 & (Self::TASK_DEAD.0 | Self::TASK_ZOMBIE.0)) != 0
    }
}

impl Default for TaskState {
    fn default() -> Self {
        Self::TASK_RUNNING
    }
}

/// Scheduling policy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum SchedPolicy {
    /// Normal (CFS) scheduling - default for most processes
    Normal = 0,
    /// Real-time FIFO - runs until it blocks or yields, no time slice
    Fifo = 1,
    /// Real-time round-robin - like FIFO but with time slice
    RoundRobin = 2,
    /// Batch processing - CFS but with lower priority, for CPU-intensive non-interactive
    Batch = 3,
    /// Idle scheduling - only runs when system is truly idle
    Idle = 5,
}

impl SchedPolicy {
    /// Check if this is a real-time policy
    pub const fn is_realtime(&self) -> bool {
        matches!(self, Self::Fifo | Self::RoundRobin)
    }

    /// Check if this is a normal/fair policy
    pub const fn is_fair(&self) -> bool {
        matches!(self, Self::Normal | Self::Batch)
    }

    /// Get default time slice in ticks for this policy
    pub const fn default_time_slice(&self) -> u32 {
        match self {
            Self::RoundRobin => 10, // 100ms at 100Hz
            _ => 0,                 // No time slice for other policies
        }
    }
}

impl Default for SchedPolicy {
    fn default() -> Self {
        Self::Normal
    }
}

impl TryFrom<u32> for SchedPolicy {
    type Error = ();

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Normal),
            1 => Ok(Self::Fifo),
            2 => Ok(Self::RoundRobin),
            3 => Ok(Self::Batch),
            5 => Ok(Self::Idle),
            _ => Err(()),
        }
    }
}

/// CPU set for affinity - supports up to 256 CPUs
///
/// This is a bitmask where each bit represents whether a CPU is allowed.
/// Bit N is set if CPU N is in the set.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct CpuSet {
    bits: [u64; 4], // 256 bits total
}

impl CpuSet {
    /// Create an empty CPU set (no CPUs allowed)
    pub const fn empty() -> Self {
        Self { bits: [0; 4] }
    }

    /// Create a CPU set with all CPUs allowed
    pub const fn all() -> Self {
        Self {
            bits: [u64::MAX; 4],
        }
    }

    /// Create a CPU set with only the specified CPU
    pub const fn single(cpu: u32) -> Self {
        let mut set = Self::empty();
        if cpu < 256 {
            let word = (cpu / 64) as usize;
            let bit = cpu % 64;
            set.bits[word] = 1u64 << bit;
        }
        set
    }

    /// Create a CPU set from a range of CPUs (inclusive)
    pub fn range(start: u32, end: u32) -> Self {
        let mut set = Self::empty();
        for cpu in start..=end {
            set.set(cpu);
        }
        set
    }

    /// Add a CPU to the set
    pub fn set(&mut self, cpu: u32) {
        if cpu < 256 {
            let word = (cpu / 64) as usize;
            let bit = cpu % 64;
            self.bits[word] |= 1u64 << bit;
        }
    }

    /// Remove a CPU from the set
    pub fn clear(&mut self, cpu: u32) {
        if cpu < 256 {
            let word = (cpu / 64) as usize;
            let bit = cpu % 64;
            self.bits[word] &= !(1u64 << bit);
        }
    }

    /// Check if a CPU is in the set
    pub const fn is_set(&self, cpu: u32) -> bool {
        if cpu >= 256 {
            return false;
        }
        let word = (cpu / 64) as usize;
        let bit = cpu % 64;
        (self.bits[word] & (1u64 << bit)) != 0
    }

    /// Check if the set is empty
    pub const fn is_empty(&self) -> bool {
        self.bits[0] == 0 && self.bits[1] == 0 && self.bits[2] == 0 && self.bits[3] == 0
    }

    /// Get the first CPU in the set
    pub const fn first_set(&self) -> Option<u32> {
        let mut i = 0;
        while i < 4 {
            if self.bits[i] != 0 {
                return Some(i as u32 * 64 + self.bits[i].trailing_zeros());
            }
            i += 1;
        }
        None
    }

    /// Get the number of CPUs in the set
    pub const fn count(&self) -> u32 {
        self.bits[0].count_ones()
            + self.bits[1].count_ones()
            + self.bits[2].count_ones()
            + self.bits[3].count_ones()
    }

    /// Intersect with another CPU set
    pub fn and(&self, other: &Self) -> Self {
        Self {
            bits: [
                self.bits[0] & other.bits[0],
                self.bits[1] & other.bits[1],
                self.bits[2] & other.bits[2],
                self.bits[3] & other.bits[3],
            ],
        }
    }

    /// Union with another CPU set
    pub fn or(&self, other: &Self) -> Self {
        Self {
            bits: [
                self.bits[0] | other.bits[0],
                self.bits[1] | other.bits[1],
                self.bits[2] | other.bits[2],
                self.bits[3] | other.bits[3],
            ],
        }
    }

    /// Get the raw bits
    pub const fn as_bits(&self) -> &[u64; 4] {
        &self.bits
    }

    /// Create from raw bits
    pub const fn from_bits(bits: [u64; 4]) -> Self {
        Self { bits }
    }
}

impl Default for CpuSet {
    fn default() -> Self {
        Self::all()
    }
}

impl core::fmt::Debug for CpuSet {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "CpuSet[")?;
        let mut first = true;
        for cpu in 0..256u32 {
            if self.is_set(cpu) {
                if !first {
                    write!(f, ",")?;
                }
                write!(f, "{}", cpu)?;
                first = false;
            }
        }
        write!(f, "]")
    }
}

/// Nice value to weight conversion table
///
/// Weight = 1024 / 1.25^nice for nice in -20..+19
/// This gives ~10% CPU difference per nice level.
pub static NICE_TO_WEIGHT: [u64; 40] = [
    /* -20 */ 88761, 71755, 56483, 46273, 36291, /* -15 */ 29154, 23254, 18705, 14949,
    11916, /* -10 */ 9548, 7620, 6100, 4904, 3906, /*  -5 */ 3121, 2501, 1991, 1586,
    1277, /*   0 */ 1024, 820, 655, 526, 423, /*   5 */ 335, 272, 215, 172, 137,
    /*  10 */ 110, 87, 70, 56, 45, /*  15 */ 36, 29, 23, 18, 15,
];

/// Default weight (nice 0)
pub const NICE_0_WEIGHT: u64 = 1024;

/// Convert nice value to weight
pub fn nice_to_weight(nice: i8) -> u64 {
    let idx = (nice as i16 + 20).clamp(0, 39) as usize;
    NICE_TO_WEIGHT[idx]
}

/// Real-time priority range
pub const RT_PRIO_MIN: u8 = 1;
pub const RT_PRIO_MAX: u8 = 99;

/// Default time slice for RR tasks in nanoseconds
pub const RR_TIME_SLICE_NS: u64 = 100_000_000; // 100ms

/// Scheduler tick period in nanoseconds
pub const TICK_NS: u64 = 10_000_000; // 10ms (100Hz)

/// Context trait - represents saved CPU state (architecture-specific)
pub trait Context: Clone + Default + Send {
    /// Create a new context for a task
    ///
    /// - `entry`: The function the task will start executing
    /// - `stack_top`: The top of the task's kernel stack
    /// - `arg`: Argument to pass to the entry function
    fn new(entry: fn(usize) -> !, stack_top: usize, arg: usize) -> Self;

    /// Get the stack pointer from this context
    fn stack_pointer(&self) -> usize;
}

/// Context switch operations (provided by architecture crate)
pub trait ContextSwitch {
    /// Context type
    type Context: Context;

    /// Perform a context switch from `old` to `new`
    ///
    /// # Safety
    /// - Both contexts must be valid
    /// - The old context will be saved and the new context will be restored
    unsafe fn switch(old: *mut Self::Context, new: *const Self::Context);
}

/// Scheduling class trait - each class (RT, Fair, Idle) implements this
///
/// The scheduler iterates through classes in priority order (RT > Fair > Idle)
/// to find the next task to run.
pub trait SchedClass: Send + Sync {
    /// Get the name of this scheduling class
    fn name(&self) -> &'static str;

    /// Enqueue a task that has become runnable
    ///
    /// Called when:
    /// - A task is created
    /// - A blocked task wakes up
    /// - A running task is preempted
    fn enqueue_task(&self, rq: &mut dyn RunQueueOps, pid: Pid);

    /// Dequeue a task (it's leaving the runnable state)
    ///
    /// Called when:
    /// - A task blocks
    /// - A task exits
    /// - A task is being moved to another run queue
    fn dequeue_task(&self, rq: &mut dyn RunQueueOps, pid: Pid);

    /// Pick the next task to run from this class
    ///
    /// Returns None if no tasks from this class are runnable.
    fn pick_next_task(&self, rq: &mut dyn RunQueueOps) -> Option<Pid>;

    /// Put the previously running task back (it was preempted or yielded)
    ///
    /// This may or may not re-enqueue the task depending on policy.
    fn put_prev_task(&self, rq: &mut dyn RunQueueOps, pid: Pid);

    /// Handle a scheduler tick for the currently running task
    ///
    /// Returns true if the task should be preempted.
    fn tick(&self, rq: &mut dyn RunQueueOps, pid: Pid) -> bool;

    /// Check if a waking task should preempt the current task
    ///
    /// Returns true if `waking` should preempt `curr`.
    fn check_preempt_curr(&self, rq: &dyn RunQueueOps, waking: Pid, curr: Pid) -> bool;
}

/// Run queue operations trait - abstraction over the per-CPU run queue
///
/// This allows scheduling classes to manipulate the run queue without
/// knowing its concrete implementation.
pub trait RunQueueOps {
    /// Get the current CPU ID
    fn cpu(&self) -> u32;

    /// Get the number of running tasks
    fn nr_running(&self) -> u32;

    /// Get current run queue clock (nanoseconds)
    fn clock(&self) -> u64;

    /// Update the clock
    fn update_clock(&mut self, now: u64);

    /// Get task state
    fn get_task_state(&self, pid: Pid) -> Option<TaskState>;

    /// Get task policy
    fn get_task_policy(&self, pid: Pid) -> Option<SchedPolicy>;

    /// Get task RT priority
    fn get_task_rt_priority(&self, pid: Pid) -> Option<u8>;

    /// Get task vruntime
    fn get_task_vruntime(&self, pid: Pid) -> Option<u64>;

    /// Set task vruntime
    fn set_task_vruntime(&mut self, pid: Pid, vruntime: u64);

    /// Get task weight
    fn get_task_weight(&self, pid: Pid) -> Option<u64>;

    /// Get task time slice (for RR)
    fn get_task_time_slice(&self, pid: Pid) -> Option<u32>;

    /// Set task time slice
    fn set_task_time_slice(&mut self, pid: Pid, slice: u32);

    /// Get task exec start time
    fn get_task_exec_start(&self, pid: Pid) -> Option<u64>;

    /// Set task exec start time
    fn set_task_exec_start(&mut self, pid: Pid, start: u64);

    /// Get the minimum vruntime (for CFS)
    fn min_vruntime(&self) -> u64;

    /// Set the minimum vruntime
    fn set_min_vruntime(&mut self, vruntime: u64);

    /// Increment running task count
    fn inc_nr_running(&mut self);

    /// Decrement running task count
    fn dec_nr_running(&mut self);
}
