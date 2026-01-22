//! Per-CPU data structures
//!
//! Each CPU has its own private data area for thread-local state.

use crate::MAX_CPUS;
use core::sync::atomic::{AtomicU32, Ordering};

/// Per-CPU data structure
///
/// This structure holds CPU-specific state that doesn't require
/// synchronization when accessed from that CPU.
#[repr(C)]
pub struct PerCpu {
    /// Self pointer (for fast access via segment register)
    pub self_ptr: *mut PerCpu,

    /// CPU ID (0 = BSP, 1+ = APs)
    pub cpu_id: u32,

    /// APIC ID (physical ID from hardware)
    pub apic_id: u32,

    /// Is this CPU online?
    pub online: bool,

    /// Preemption disable count
    /// 0 = preemptible, >0 = preemption disabled
    pub preempt_count: u32,

    /// Interrupt nesting level
    pub irq_count: u32,

    /// Current thread pointer (opaque)
    pub current_thread: u64,

    /// Idle thread pointer (opaque)
    pub idle_thread: u64,

    /// Kernel stack pointer
    pub kernel_stack: u64,

    /// TSS pointer (x86_64 specific)
    pub tss: u64,

    /// Per-CPU statistics
    pub stats: CpuStats,
}

impl PerCpu {
    /// Create a new per-CPU data structure
    pub const fn new(cpu_id: u32) -> Self {
        PerCpu {
            self_ptr: core::ptr::null_mut(),
            cpu_id,
            apic_id: 0,
            online: false,
            preempt_count: 0,
            irq_count: 0,
            current_thread: 0,
            idle_thread: 0,
            kernel_stack: 0,
            tss: 0,
            stats: CpuStats::new(),
        }
    }

    /// Initialize the self pointer
    pub fn init(&mut self) {
        self.self_ptr = self as *mut PerCpu;
    }

    /// Disable preemption
    #[inline]
    pub fn preempt_disable(&mut self) {
        self.preempt_count += 1;
    }

    /// Enable preemption
    #[inline]
    pub fn preempt_enable(&mut self) {
        if self.preempt_count > 0 {
            self.preempt_count -= 1;
        }
    }

    /// Check if preemption is enabled
    #[inline]
    pub fn preemptible(&self) -> bool {
        self.preempt_count == 0 && self.irq_count == 0
    }

    /// Enter interrupt context
    #[inline]
    pub fn irq_enter(&mut self) {
        self.irq_count += 1;
    }

    /// Exit interrupt context
    #[inline]
    pub fn irq_exit(&mut self) {
        if self.irq_count > 0 {
            self.irq_count -= 1;
        }
    }

    /// Check if in interrupt context
    #[inline]
    pub fn in_interrupt(&self) -> bool {
        self.irq_count > 0
    }
}

/// Per-CPU statistics
#[derive(Clone, Copy)]
pub struct CpuStats {
    /// Number of context switches
    pub context_switches: u64,
    /// Number of interrupts handled
    pub interrupts: u64,
    /// Number of syscalls handled
    pub syscalls: u64,
    /// Idle time in ticks
    pub idle_ticks: u64,
    /// User time in ticks
    pub user_ticks: u64,
    /// System time in ticks
    pub system_ticks: u64,
}

impl CpuStats {
    pub const fn new() -> Self {
        CpuStats {
            context_switches: 0,
            interrupts: 0,
            syscalls: 0,
            idle_ticks: 0,
            user_ticks: 0,
            system_ticks: 0,
        }
    }
}

/// Global array of per-CPU data
static mut PERCPU_DATA: [PerCpu; MAX_CPUS] = {
    const INIT: PerCpu = PerCpu::new(0);
    [INIT; MAX_CPUS]
};

/// Number of initialized CPUs
static CPU_COUNT: AtomicU32 = AtomicU32::new(0);

/// Initialize per-CPU data for a CPU
///
/// # Safety
/// Must be called once per CPU during boot.
pub unsafe fn init_percpu(cpu_id: u32, apic_id: u32) {
    if (cpu_id as usize) < MAX_CPUS {
        let percpu = &mut PERCPU_DATA[cpu_id as usize];
        percpu.cpu_id = cpu_id;
        percpu.apic_id = apic_id;
        percpu.init();
        percpu.online = true;

        CPU_COUNT.fetch_max(cpu_id + 1, Ordering::SeqCst);
    }
}

/// Get per-CPU data for a specific CPU
///
/// # Safety
/// The CPU must be initialized.
pub unsafe fn get_percpu(cpu_id: u32) -> &'static mut PerCpu {
    &mut PERCPU_DATA[cpu_id as usize]
}

/// Get the number of CPUs
pub fn get_cpu_count() -> u32 {
    CPU_COUNT.load(Ordering::Relaxed)
}
