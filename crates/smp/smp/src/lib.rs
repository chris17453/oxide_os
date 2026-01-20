//! SMP (Symmetric Multiprocessing) support for OXIDE OS
//!
//! This crate provides multi-core support including:
//! - Per-CPU data structures
//! - CPU enumeration and boot
//! - Inter-processor interrupts (IPI)
//! - TLB shootdowns

#![no_std]
#![allow(unsafe_op_in_unsafe_fn)]

extern crate alloc;

pub mod percpu;
pub mod cpu;
pub mod ipi;
pub mod tlb;

pub use percpu::PerCpu;
pub use cpu::{CpuId, CpuState, cpu_count, current_cpu, boot_ap};
pub use ipi::{IpiTarget, send_ipi};
pub use tlb::tlb_shootdown;

/// Maximum number of CPUs supported
pub const MAX_CPUS: usize = 256;
