//! CPU enumeration and management
//!
//! Handles CPU discovery, state tracking, and AP boot coordination.

use crate::MAX_CPUS;
use crate::percpu;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use arch_traits::Arch;
use arch_x86_64 as arch;

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
/// On x86_64, this reads from the GS segment or APIC ID.
/// For now, returns 0 (single CPU assumption until AP boot).
pub fn current_cpu() -> CpuId {
    arch::cpu_id().unwrap_or(0)
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

/// Get the APIC ID for a CPU
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
/// This is architecture-specific. On x86_64, it involves:
/// 1. Send INIT IPI
/// 2. Wait 10ms
/// 3. Send SIPI with startup vector
/// 4. Wait 200us
/// 5. Send second SIPI
///
/// The trampoline code must be set up before calling this function.
pub fn boot_ap(cpu_id: CpuId, trampoline_page: u8) -> Result<(), &'static str> {
    let state = get_cpu_state(cpu_id);

    if state == CpuState::NotPresent {
        return Err("CPU not present");
    }

    if state == CpuState::Online {
        return Err("CPU already online");
    }

    // Get the APIC ID for the target CPU
    let apic_id = get_apic_id(cpu_id).ok_or("CPU has no APIC ID")?;

    unsafe {
        CPU_INFO[cpu_id as usize].state = CpuState::Starting;
    }

    // Send INIT IPI to reset the AP
    arch_x86_64::apic::send_ipi(
        apic_id as u8,
        0, // Vector ignored for INIT
        arch_x86_64::apic::DeliveryMode::Init,
        arch_x86_64::apic::DestShorthand::None,
    );

    // Wait 10ms for INIT to take effect
    arch_x86_64::delay_ms(10);

    // Send first SIPI with startup vector (page number where trampoline is located)
    arch_x86_64::apic::send_ipi(
        apic_id as u8,
        trampoline_page, // Startup vector = page number (e.g., 0x08 = 0x8000)
        arch_x86_64::apic::DeliveryMode::Startup,
        arch_x86_64::apic::DestShorthand::None,
    );

    // Wait 200 microseconds
    arch_x86_64::delay_us(200);

    // Send second SIPI (per Intel spec, for reliability)
    arch_x86_64::apic::send_ipi(
        apic_id as u8,
        trampoline_page,
        arch_x86_64::apic::DeliveryMode::Startup,
        arch_x86_64::apic::DestShorthand::None,
    );

    // Wait for AP to come online (with timeout)
    let timeout_ms = 1000; // 1 second timeout
    let start = arch_x86_64::read_tsc();
    let tsc_per_ms = arch_x86_64::tsc_frequency() / 1000;
    let timeout_tsc = start + (timeout_ms * tsc_per_ms);

    loop {
        if get_cpu_state(cpu_id) == CpuState::Online {
            return Ok(());
        }

        if arch_x86_64::read_tsc() > timeout_tsc {
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
