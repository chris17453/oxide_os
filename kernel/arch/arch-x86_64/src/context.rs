//! Context switch implementation for x86_64
//!
//! Provides the CPU context structure and preemptive context switching
//! via timer interrupts.

use crate::exceptions::InterruptContext;
use core::mem::size_of;
use sched_traits::Context;

/// x86_64 thread context for preemptive multitasking
///
/// This stores the RSP that points to an InterruptContext on the thread's stack.
/// When a timer interrupt occurs, the interrupt handler saves all registers
/// on the stack, then the scheduler can switch by changing which stack we
/// restore from.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct X86_64Context {
    /// Stack pointer pointing to saved InterruptContext
    pub rsp: u64,
}

impl Default for X86_64Context {
    fn default() -> Self {
        Self { rsp: 0 }
    }
}

impl Context for X86_64Context {
    fn new(entry: fn(usize) -> !, stack_top: usize, arg: usize) -> Self {
        // Create an InterruptContext on the thread's stack
        // This will be "restored" when we first switch to this thread

        // Align stack to 16 bytes
        let stack_top = stack_top & !0xF;

        // Reserve space for InterruptContext
        let context_ptr = (stack_top - size_of::<InterruptContext>()) as *mut InterruptContext;

        // Initialize the context
        let context = InterruptContext::new(entry, stack_top, arg);

        // Write context to stack
        unsafe {
            core::ptr::write(context_ptr, context);
        }

        Self {
            rsp: context_ptr as u64,
        }
    }

    fn stack_pointer(&self) -> usize {
        self.rsp as usize
    }
}

impl X86_64Context {
    /// Create a context from an existing RSP (used during context switch)
    pub fn from_rsp(rsp: u64) -> Self {
        Self { rsp }
    }

    /// Get the RSP value
    pub fn rsp(&self) -> u64 {
        self.rsp
    }

    /// Set the RSP value
    pub fn set_rsp(&mut self, rsp: u64) {
        self.rsp = rsp;
    }
}
