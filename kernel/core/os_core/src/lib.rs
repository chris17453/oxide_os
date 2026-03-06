//! OXIDE Core - Fundamental types and utilities
//!
//! This crate provides core types used throughout the OXIDE kernel.
//! All types are `#![no_std]` compatible.

#![no_std]

pub mod addr;
pub mod creds;
pub mod sync;
pub mod time;

pub use addr::{PhysAddr, VirtAddr};
pub use creds::{current_uid_gid, register_creds_provider};
pub use sync::{Mutex, MutexGuard, KernelMutex, KernelMutexGuard, register_preempt_hooks, register_preempt_control, allow_kernel_preempt, disallow_kernel_preempt, register_arch_ops, user_access_begin, user_access_end, wait_for_interrupt, enable_interrupts, disable_interrupts};
pub use sync::{register_port_io, inb, outb, inw, outw, inl, outl};
pub use sync::{register_sys_ops, read_msr, write_msr, read_tsc, cpuid, memory_fence, read_fence};
pub use sync::{register_tlb_ops, tlb_flush, tlb_flush_all};
pub use sync::{register_page_table_root_ops, read_page_table_root, write_page_table_root};
pub use sync::{register_elf_machine, elf_machine};
pub use sync::{register_smp_ops, smp_send_ipi_to, smp_send_ipi_broadcast, smp_send_ipi_self, smp_boot_ap, smp_cpu_id, smp_delay_us, smp_monotonic_counter, smp_monotonic_freq};
pub use time::{register_wall_clock, wall_clock_secs, register_tick_source, now_ns, ticks, ns_per_tick, register_hires_monotonic, monotonic_secs_ns, register_hires_wall_clock, realtime_secs_ns};
