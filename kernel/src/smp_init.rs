//! SMP (Symmetric Multi-Processing) initialization callbacks for the OXIDE kernel.

use arch_traits::Arch;
use arch_x86_64 as arch;

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

    // Start local APIC timer for this CPU (matches BSP frequency)
    arch::start_timer(100);

    // Enable interrupts so we can receive IPIs for TLB shootdown
    arch::X86_64::enable_interrupts();

    // AP is now online - enter scheduler idle loop
    loop {
        sched::yield_current();
        unsafe { core::arch::asm!("hlt"); }
    }
}
