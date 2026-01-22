//! Scheduler implementation for OXIDE OS
//!
//! Provides a round-robin scheduler for kernel threads.

#![no_std]

extern crate alloc;

mod round_robin;
mod smp;
mod thread;

pub use round_robin::RoundRobinScheduler;
pub use sched_traits::*;
pub use smp::{PerCpuScheduler, SmpScheduler};
pub use thread::KernelThread;
