//! Scheduler implementation for EFFLUX OS
//!
//! Provides a round-robin scheduler for kernel threads.

#![no_std]

extern crate alloc;

mod thread;
mod round_robin;

pub use thread::KernelThread;
pub use round_robin::RoundRobinScheduler;
pub use efflux_sched_traits::*;
