//! Kernel thread implementation

use os_core::VirtAddr;
use sched_traits::{Context, Priority, Thread, ThreadId, ThreadState, DEFAULT_PRIORITY};

/// Kernel thread structure
///
/// Generic over the context type to support different architectures.
pub struct KernelThread<C: Context> {
    /// Thread identifier
    tid: ThreadId,
    /// Current state
    state: ThreadState,
    /// Priority (0 = highest)
    priority: Priority,
    /// Kernel stack top address
    kernel_stack_top: VirtAddr,
    /// Kernel stack size
    kernel_stack_size: usize,
    /// Saved context
    context: C,
}

impl<C: Context> KernelThread<C> {
    /// Create a new kernel thread
    ///
    /// - `tid`: Unique thread identifier
    /// - `entry`: Function to execute
    /// - `stack_top`: Top of the kernel stack
    /// - `stack_size`: Size of the kernel stack
    /// - `arg`: Argument to pass to entry function
    pub fn new(
        tid: ThreadId,
        entry: fn(usize) -> !,
        stack_top: VirtAddr,
        stack_size: usize,
        arg: usize,
    ) -> Self {
        let context = C::new(entry, stack_top.as_usize(), arg);

        Self {
            tid,
            state: ThreadState::Ready,
            priority: DEFAULT_PRIORITY,
            kernel_stack_top: stack_top,
            kernel_stack_size: stack_size,
            context,
        }
    }

    /// Create a new kernel thread with custom priority
    pub fn with_priority(
        tid: ThreadId,
        entry: fn(usize) -> !,
        stack_top: VirtAddr,
        stack_size: usize,
        arg: usize,
        priority: Priority,
    ) -> Self {
        let mut thread = Self::new(tid, entry, stack_top, stack_size, arg);
        thread.priority = priority;
        thread
    }

    /// Get the kernel stack size
    pub fn kernel_stack_size(&self) -> usize {
        self.kernel_stack_size
    }
}

impl<C: Context> Thread for KernelThread<C> {
    type Context = C;

    fn tid(&self) -> ThreadId {
        self.tid
    }

    fn state(&self) -> ThreadState {
        self.state
    }

    fn set_state(&mut self, state: ThreadState) {
        self.state = state;
    }

    fn priority(&self) -> Priority {
        self.priority
    }

    fn context(&self) -> &Self::Context {
        &self.context
    }

    fn context_mut(&mut self) -> &mut Self::Context {
        &mut self.context
    }

    fn kernel_stack_top(&self) -> VirtAddr {
        self.kernel_stack_top
    }
}
