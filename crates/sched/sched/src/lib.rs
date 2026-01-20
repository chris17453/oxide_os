//! Scheduler implementation for OXIDE OS
//!
//! Provides a round-robin scheduler for kernel threads.

#![no_std]

extern crate alloc;

mod thread;
mod round_robin;
mod smp;

pub use thread::KernelThread;
pub use round_robin::RoundRobinScheduler;
pub use smp::{SmpScheduler, PerCpuScheduler};
pub use sched_traits::*;
