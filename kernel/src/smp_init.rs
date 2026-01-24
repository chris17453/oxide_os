//! SMP (Symmetric Multi-Processing) initialization callbacks for the OXIDE kernel.

use arch_traits::Arch;
use arch_x86_64 as arch;

/// AP initialization callback - called when an Application Processor starts
///
/// This is called from the AP boot trampoline with the AP's APIC ID.
pub fn ap_init_callback(apic_id: u8) -> ! {
    // Find which CPU ID corresponds to this APIC ID and mark it online
    for cpu_id in 0..smp::MAX_CPUS as u32 {
        if let Some(id) = smp::cpu::get_apic_id(cpu_id) {
            if id == apic_id as u32 {
                smp::cpu::set_cpu_online(cpu_id);
                break;
            }
        }
    }

    // Enable interrupts so we can receive IPIs for TLB shootdown
    arch::X86_64::enable_interrupts();

    // AP is now online - enter idle loop
    // In a full system, this would call the scheduler
    loop {
        unsafe {
            core::arch::asm!("hlt");
        }
    }
}
