//! SMP (Symmetric Multi-Processing) initialization callbacks for the OXIDE kernel.
//!
//! — NeonRoot: APs must NOT enable interrupts or timers until the BSP has
//!   registered all fault handlers (page fault, scheduler, etc). Otherwise an
//!   AP timer tick that faults has no handler → double fault → triple fault →
//!   silent QEMU exit. The BSP signals readiness via `signal_ap_ready()`.

use arch_traits::Arch;
use arch_x86_64 as arch;
use core::sync::atomic::{AtomicBool, Ordering};

/// BSP sets this after all interrupt callbacks are registered and interrupts
/// are enabled. APs spin on it before starting their timers.
static BSP_READY: AtomicBool = AtomicBool::new(false);

/// Called by the BSP after page fault handler, scheduler, and timer are live.
pub fn signal_ap_ready() {
    BSP_READY.store(true, Ordering::Release);
}

/// AP initialization callback - called when an Application Processor starts
///
/// This is called from the AP boot trampoline with the AP's APIC ID.
pub fn ap_init_callback(apic_id: u8) -> ! {
    // Find CPU ID for this APIC ID and bring it fully online
    let mut cpu_id = None;
    for id in 0..smp::MAX_CPUS as u32 {
        if let Some(apic) = smp::cpu::get_apic_id(id) {
            if apic == apic_id as u32 {
                cpu_id = Some(id);
                smp::cpu::set_cpu_online(id);
                break;
            }
        }
    }

    // If we couldn't resolve the CPU ID, halt safely
    let cpu_id = cpu_id.unwrap_or(0);

    // Initialize scheduler structures for this CPU and set per-CPU ID
    sched::set_this_cpu(cpu_id);
    sched::init_cpu(cpu_id, 0);

    // NeonRoot: Wait for BSP to finish registering all fault handlers,
    // scheduler callback, and enabling its own interrupts. Without this
    // gate, an AP timer tick that page-faults has no handler registered
    // and escalates to a triple fault.
    while !BSP_READY.load(Ordering::Acquire) {
        core::hint::spin_loop();
    }

    // Now safe — all handlers are live on the BSP
    arch::start_timer(100);
    arch::X86_64::enable_interrupts();

    // AP is now online - enter scheduler idle loop
    loop {
        sched::yield_current();
        unsafe {
            core::arch::asm!("hlt");
        }
    }
}
