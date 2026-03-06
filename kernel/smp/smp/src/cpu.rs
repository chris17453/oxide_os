//! CPU enumeration and management
//!
//! Handles CPU discovery, state tracking, and AP boot coordination.
//! — NeonRoot: ALL arch-specific logic lives in the arch crate's SmpOps impl.
//! This module is pure state management — no APIC, no TSC, no x86 anything.

use crate::MAX_CPUS;
use crate::percpu;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// CPU identifier type
pub type CpuId = u32;

/// CPU state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CpuState {
    /// CPU not present
    NotPresent = 0,
    /// CPU present but not started
    Present = 1,
    /// CPU is starting up
    Starting = 2,
    /// CPU is online and running
    Online = 3,
    /// CPU is being taken offline
    GoingOffline = 4,
    /// CPU is offline
    Offline = 5,
}

/// CPU information
pub struct CpuInfo {
    /// CPU state
    pub state: CpuState,
    /// APIC ID (x86) or equivalent hardware ID
    pub apic_id: u32,
    /// Is this the bootstrap processor?
    pub is_bsp: bool,
}

impl CpuInfo {
    pub const fn new() -> Self {
        CpuInfo {
            state: CpuState::NotPresent,
            apic_id: 0,
            is_bsp: false,
        }
    }
}

/// Global CPU information array
static mut CPU_INFO: [CpuInfo; MAX_CPUS] = {
    const INIT: CpuInfo = CpuInfo::new();
    [INIT; MAX_CPUS]
};

/// Number of CPUs detected
static NUM_CPUS: AtomicU32 = AtomicU32::new(0);

/// Number of CPUs online
static CPUS_ONLINE: AtomicU32 = AtomicU32::new(0);

/// BSP has finished initialization
static BSP_DONE: AtomicBool = AtomicBool::new(false);

/// Register a CPU as present
///
/// # Safety
/// Must be called during early boot for each CPU detected.
pub unsafe fn register_cpu(cpu_id: CpuId, apic_id: u32, is_bsp: bool) {
    if (cpu_id as usize) < MAX_CPUS {
        CPU_INFO[cpu_id as usize] = CpuInfo {
            state: CpuState::Present,
            apic_id,
            is_bsp,
        };
        NUM_CPUS.fetch_max(cpu_id + 1, Ordering::SeqCst);
    }
}

/// Get the number of CPUs detected
pub fn cpu_count() -> u32 {
    NUM_CPUS.load(Ordering::Relaxed)
}

/// Get the number of CPUs currently online
pub fn cpus_online() -> u32 {
    CPUS_ONLINE.load(Ordering::Relaxed)
}

/// Get the current CPU ID
///
/// — NeonRoot: delegates to arch SmpOps. x86 reads APIC ID, ARM reads MPIDR.
pub fn current_cpu() -> CpuId {
    // — NeonRoot: ask os_core who we are. APIC ID on x86, MPIDR on ARM — we don't care.
    os_core::smp_cpu_id().unwrap_or(0)
}

/// Mark a CPU as online
pub fn set_cpu_online(cpu_id: CpuId) {
    unsafe {
        if (cpu_id as usize) < MAX_CPUS {
            CPU_INFO[cpu_id as usize].state = CpuState::Online;
            CPUS_ONLINE.fetch_add(1, Ordering::SeqCst);
        }
    }
}

/// Mark a CPU as offline
pub fn set_cpu_offline(cpu_id: CpuId) {
    unsafe {
        if (cpu_id as usize) < MAX_CPUS {
            CPU_INFO[cpu_id as usize].state = CpuState::Offline;
            CPUS_ONLINE.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

/// Get CPU state
pub fn get_cpu_state(cpu_id: CpuId) -> CpuState {
    unsafe {
        if (cpu_id as usize) < MAX_CPUS {
            CPU_INFO[cpu_id as usize].state
        } else {
            CpuState::NotPresent
        }
    }
}

/// Check if a CPU is the BSP
pub fn is_bsp(cpu_id: CpuId) -> bool {
    unsafe {
        if (cpu_id as usize) < MAX_CPUS {
            CPU_INFO[cpu_id as usize].is_bsp
        } else {
            false
        }
    }
}

/// Reverse-map APIC ID to logical CPU ID.
///
/// — NeonRoot: Scans the CPU_INFO table to find which logical CPU owns this
/// hardware ID. Returns 0 (BSP) if not found.
pub fn cpu_id_from_apic(apic_id: u32) -> u32 {
    let count = NUM_CPUS.load(Ordering::Relaxed) as usize;
    let limit = core::cmp::min(count, MAX_CPUS);
    unsafe {
        for i in 0..limit {
            if CPU_INFO[i].state != CpuState::NotPresent && CPU_INFO[i].apic_id == apic_id {
                return i as u32;
            }
        }
    }
    0 // Fallback to BSP
}

/// Get the hardware ID (APIC ID on x86) for a CPU
pub fn get_apic_id(cpu_id: CpuId) -> Option<u32> {
    unsafe {
        if (cpu_id as usize) < MAX_CPUS {
            let info = &CPU_INFO[cpu_id as usize];
            if info.state != CpuState::NotPresent {
                Some(info.apic_id)
            } else {
                None
            }
        } else {
            None
        }
    }
}

/// Boot an Application Processor
///
/// — NeonRoot: state management is generic, the actual boot sequence
/// (INIT/SIPI on x86, PSCI on ARM) goes through os_core hooks.
/// We just call smp_boot_ap() and spin-wait with a monotonic timeout.
pub fn boot_ap(cpu_id: CpuId, trampoline_page: u8) -> Result<(), &'static str> {
    let state = get_cpu_state(cpu_id);

    if state == CpuState::NotPresent {
        return Err("CPU not present");
    }

    if state == CpuState::Online {
        return Err("CPU already online");
    }

    let apic_id = get_apic_id(cpu_id).ok_or("CPU has no APIC ID")?;

    unsafe {
        CPU_INFO[cpu_id as usize].state = CpuState::Starting;
    }

    // — NeonRoot: os_core dispatches to whatever boot protocol the arch needs
    os_core::smp_boot_ap(apic_id, trampoline_page);

    // — NeonRoot: spin-wait using os_core monotonic counter. No TSC, no APIC —
    // just the hook. 1 second timeout, then we bail.
    let timeout_ms: u64 = 1000;
    let freq = os_core::smp_monotonic_freq();
    let ticks_per_ms = freq / 1000;
    let start = os_core::smp_monotonic_counter();
    let timeout_ticks = start + (timeout_ms * ticks_per_ms);

    loop {
        if get_cpu_state(cpu_id) == CpuState::Online {
            return Ok(());
        }

        if os_core::smp_monotonic_counter() > timeout_ticks {
            unsafe {
                CPU_INFO[cpu_id as usize].state = CpuState::Present;
            }
            return Err("AP boot timeout");
        }

        core::hint::spin_loop();
    }
}

/// Signal that BSP initialization is complete
pub fn bsp_init_done() {
    BSP_DONE.store(true, Ordering::SeqCst);
}

/// Wait for BSP initialization to complete
pub fn wait_for_bsp() {
    while !BSP_DONE.load(Ordering::SeqCst) {
        core::hint::spin_loop();
    }
}

/// Initialize the BSP (bootstrap processor)
///
/// # Safety
/// Must be called once during early boot on CPU 0.
pub unsafe fn init_bsp(apic_id: u32) {
    register_cpu(0, apic_id, true);
    percpu::init_percpu(0, apic_id);
    set_cpu_online(0);
}
