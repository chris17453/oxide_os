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

    // — GraveShift: CRITICAL SMP FIX — each AP must initialize its own syscall
    // infrastructure. Without this, the first userspace syscall on this CPU:
    //   1. Has EFER.SCE unset → syscall instruction #UDs
    //   2. Has LSTAR=0 → RIP jumps to 0 on syscall
    //   3. Has KERNEL_GS_BASE=0 → swapgs loads garbage → RSP = trash → death
    // The old code only called these on the BSP, which is why SMP booted ~50%
    // of the time (tasks that never got scheduled on APs were fine).
    unsafe {
        // Set EFER.SCE, STAR, LSTAR, SFMASK for this CPU's syscall instruction
        arch::syscall::init();

        // Set KERNEL_GS_BASE to this CPU's per-CPU data slot.
        // Use current RSP as initial kernel stack — the scheduler will overwrite
        // it with the real task stack on the first context switch via set_kernel_stack().
        let boot_rsp: u64;
        core::arch::asm!("mov {}, rsp", out(reg) boot_rsp, options(nostack, nomem));
        arch::syscall::init_kernel_stack(cpu_id, boot_rsp);
    }

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

    // — GraveShift: Staggered timer start to prevent thundering herd.
    // All APs exit the spin loop simultaneously, and if they all start their
    // timers at once, all timer interrupts fire at the same instant causing
    // race conditions in the scheduler. Delay each AP by (cpu_id * 1ms) so
    // their timer phases are offset.
    for _ in 0..(cpu_id as u64 * 100_000) {
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
