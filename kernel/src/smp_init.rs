//! SMP (Symmetric Multi-Processing) initialization callbacks for the OXIDE kernel.
//!
//! — NeonRoot: APs must NOT enable interrupts or timers until the BSP has
//!   registered all fault handlers (page fault, scheduler, etc). Otherwise an
//!   AP timer tick that faults has no handler → double fault → triple fault →
//!   silent QEMU exit. The BSP signals readiness via `signal_ap_ready()`.

use arch_traits::Arch;
use crate::arch;
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

    // — WireSaint: P0.2 — Set up per-CPU IST double-fault stack now that we
    // know our logical cpu_id. gdt::init_cpu already ran in ap_entry_rust,
    // so the TSS descriptor is live. init_ap writes ist[0] (IST1) for this CPU.
    // Must happen before we enable interrupts or start the timer — otherwise
    // the first double fault fires with ist[0]=0 and triple-faults silently.
    unsafe {
        arch::init_ap(cpu_id as usize);
    }

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
        let boot_rsp: u64 = crate::arch::read_stack_pointer();
        arch::syscall::init_kernel_stack(cpu_id, boot_rsp);
    }

    // Initialize scheduler structures for this CPU and set per-CPU ID
    sched::set_this_cpu(cpu_id);
    sched::init_cpu(cpu_id, 0);

    // — WireSaint: CRITICAL SMP FIX — create a proper idle Task for this AP.
    // The BSP creates PID 0's Task in scheduler::init() with cs=0x08, ss=0x10.
    // But that Task only lives in CPU 0's BTreeMap. Without an idle Task on THIS
    // CPU's RQ, pick_next_task() returns idle PID 0 but context_switch_transaction
    // fails (get_task(0) → None), corrupting rq.curr and causing the scheduler to
    // build iret frames with stale/default contexts. Each AP needs its own PID 0
    // Task so the scheduler can save/restore idle context correctly.
    //
    // PID 0 in multiple BTreeMaps is fine — each CPU has an independent RQ.
    // The idle task uses the kernel PML4 and the AP's boot stack (no separate stack).
    let mut ap_idle = sched::Task::new_idle(
        0,                                   // PID 0 = idle
        cpu_id,                              // pinned to this AP
        os_core::PhysAddr::new(0),           // no separate kernel stack (uses boot stack)
        0,                                   // stack size = 0 (idle uses boot stack)
    );
    // — WireSaint: Idle task MUST have a fully valid context — not just selectors.
    // TaskContext::default() leaves rip=0, rsp=0. If the scheduler ever builds an
    // iret frame from these before the first timer preemption overwrites them,
    // rsp(0) - frame_size underflows → writes to 0xFFFFFFFFFFFFFF60 → instant death.
    // Defense-in-depth: set rip to the shared idle_loop and rsp to this AP's boot
    // stack. The first timer tick will overwrite these with real interrupt-frame
    // values, but at least the initial state is sane if anything goes sideways.
    ap_idle.context.cs = 0x08;
    ap_idle.context.ss = 0x10;
    ap_idle.context.rflags = 0x202; // IF + reserved bit 1
    ap_idle.context.rip = crate::scheduler::idle_loop as *const () as u64;
    // — GraveShift: boot_rsp was captured above (line 64). Reuse it for the idle
    // task's initial RSP so the scheduler has a valid kernel stack pointer if it
    // ever needs to build an iret frame for idle before the first preemption.
    let idle_boot_rsp: u64 = crate::arch::read_stack_pointer();
    ap_idle.context.rsp = idle_boot_rsp;
    sched::add_task_to_cpu(ap_idle, cpu_id);

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

    // — WireSaint: AP idle loop — use the SAME idle_loop as the BSP.
    // No duplicated logic, no drift. One idle function to rule them all.
    crate::scheduler::idle_loop();
}
