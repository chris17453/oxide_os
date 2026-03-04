//! Preemptive scheduler for the OXIDE kernel.
//!
//! This module provides the bridge between the new Linux-model scheduler
//! (in the sched crate) and the kernel's process management.
//!
//! The sched crate handles scheduling decisions (which task to run),
//! while this module handles the actual context switching using the
//! process table from the proc crate.

extern crate alloc;

use alloc::sync::Arc;
use arch_x86_64 as arch;
use mm_paging::phys_to_virt;
use os_core::PhysAddr;
use proc::ProcessMeta;
use sched::{self, SchedPolicy, Task, TaskState};
use signal::delivery::{SignalResult, determine_action};
use spin::Mutex;
use vfs;

/// Interrupt stack frame layout
/// Matches what timer_interrupt pushes in exceptions.rs
#[repr(C)]
pub struct InterruptFrame {
    // Pushed by our handler
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rbp: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,
    // Pushed by CPU on interrupt
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

/// Handle reschedule IPI from another CPU
///
/// Called when a remote CPU wakes a task that should run on this CPU.
/// Sets need_resched flag so the next timer tick (or explicit schedule() call)
/// will switch to the newly-woken task.
/// — NeonRoot: Cross-CPU scheduler coordination
fn handle_reschedule_ipi(_vector: u8) {
    // NeonRoot: Mark this CPU for reschedule — next timer tick will context switch
    sched::set_need_resched();

    // Send EOI to APIC (acknowledge interrupt)
    arch::apic::end_of_interrupt();
}

/// Idle loop - runs when no other tasks are runnable
///
/// This function uses HLT to sleep the CPU until an interrupt arrives,
/// preventing 100% CPU usage when the system is idle.
///
/// CRITICAL: Must call allow_kernel_preempt() before HLT. The idle loop
/// runs in kernel mode (CS=0x08), and scheduler_tick() skips context
/// switches for kernel-mode tasks unless kernel_preempt_ok is set.
/// Without this, once idle starts running, the timer interrupt can never
/// switch to a woken user process — the system deadlocks.
pub extern "C" fn idle_loop() -> ! {
    loop {
        // Allow the timer interrupt to preempt us and switch to a runnable task
        arch::allow_kernel_preempt();

        // Enable interrupts and halt atomically
        // The CPU will wake up on the next interrupt (timer, keyboard, etc.)
        unsafe {
            core::arch::asm!(
                "sti", // Enable interrupts
                "hlt", // Halt until interrupt
                options(nomem, nostack)
            );
        }

        // Note: don't clear the preempt flag here — the timer interrupt's
        // scheduler_tick() clears it after checking. We re-set it on every
        // iteration so it's always true when HLT is interrupted.
    }
}

/// Initialize the scheduler for the current CPU
///
/// Should be called early during kernel initialization.
pub fn init() {
    // Initialize the scheduler for CPU 0 with idle task PID 0
    sched::set_this_cpu(0);
    sched::init_cpu(0, 0);

    // Create a real idle task with PID 0
    // The idle task doesn't need a real kernel stack since it runs on the BSP stack
    // We use a placeholder address that won't be used for context switches to idle
    let meta = ProcessMeta::new_kernel();
    let idle_meta = Arc::new(Mutex::new(meta));

    // Create idle task with ProcessMeta
    let mut idle_task = Task::new_idle_with_meta(
        0,                // PID 0 is the idle task
        0,                // CPU 0
        PhysAddr::new(0), // No separate kernel stack needed
        0,                // Stack size (idle uses BSP stack)
        idle_meta,
    );

    // — WireSaint: BSP idle MUST have a fully valid context — rip AND rsp.
    // TaskContext::default() leaves rsp=0. If the scheduler ever switches to idle
    // before a timer tick overwrites the context, the frame builder computes
    // rsp(0) - frame_size → underflow to 0xFFFFFFFFFFFFFF60 → writes the iretq
    // frame to random kernel memory → cascading corruption. Same fix as AP idle
    // in smp_init.rs — capture boot RSP so the initial context is sane.
    let boot_rsp: u64;
    unsafe {
        core::arch::asm!("mov {}, rsp", out(reg) boot_rsp, options(nostack, nomem));
    }
    let mut ctx = idle_task.context;
    ctx.rip = idle_loop as *const () as u64;
    ctx.rsp = boot_rsp;
    ctx.rflags = 0x202; // IF (interrupts enabled) + reserved bit 1
    ctx.cs = 0x08; // Kernel code segment
    ctx.ss = 0x10; // Kernel data segment
    idle_task.context = ctx;

    // Add the idle task to the scheduler
    sched::add_task(idle_task);

    // NeonRoot: Register reschedule IPI handler for cross-CPU wakeups
    unsafe {
        smp::ipi::register_handler(smp::ipi::vector::RESCHEDULE, handle_reschedule_ipi);
    }
}

/// Add a process to the scheduler
///
/// DEPRECATED: kernel_fork now creates Tasks directly.
/// This function is kept for legacy compatibility but does nothing.
pub fn add_process(_pid: u32) {
    // In the unified model, Tasks are created directly with ProcessMeta
    // by kernel_fork. This function is no longer needed.
}

/// Remove a process from the scheduler
///
/// Called when a process exits.
pub fn remove_process(pid: u32) {
    #[cfg(feature = "debug-proc")]
    unsafe {
        os_log::write_str_raw("[WAIT] reap pid=");
        if pid == 0 {
            os_log::write_byte_raw(b'0');
        } else {
            let mut n = pid;
            let mut buf = [0u8; 10];
            let mut pos = 0;
            while n > 0 {
                buf[pos] = b'0' + (n % 10) as u8;
                n /= 10;
                pos += 1;
            }
            for i in (0..pos).rev() {
                os_log::write_byte_raw(buf[i]);
            }
        }
        os_log::write_str_raw("\n");
    }
    sched::remove_task(pid);
}

/// Scheduler tick callback - called from timer interrupt at 100Hz
///
/// Implements preemptive scheduling using the sched crate.
/// Returns the RSP to restore (may be different if we switched processes).
pub fn scheduler_tick(current_rsp: u64) -> u64 {
    // — CrashBloom: PML4[256] CANARY — check CURRENT page tables every tick.
    // If the currently-loaded CR3 has corrupted kernel entries, we're living
    // on borrowed time. The ISR itself runs from kernel memory, so if we got
    // here, PML4[256] WAS valid at ISR entry. But checking it catches the case
    // where corruption happened between ticks — the NEXT code path that touches
    // an unmapped kernel address would triple-fault. Catch it here first.
    // Also check every Nth tick for the RUNNING process's PML4 frame being freed.
    unsafe {
        let golden = crate::globals::KERNEL_PML4_256_ENTRY;
        if golden != 0 {
            let cr3: u64;
            core::arch::asm!("mov {}, cr3", out(reg) cr3, options(nostack, preserves_flags));
            let pml4_virt = mm_paging::phys_to_virt(PhysAddr::new(cr3));
            let current_entry = core::ptr::read_volatile(
                pml4_virt.as_ptr::<u64>().add(256)
            );
            if current_entry != golden {
                let pid = sched::current_pid_lockfree().unwrap_or(0);
                os_log::write_str_raw("[PML4-LIVE-CORRUPT] pid=");
                os_log::write_u32_raw(pid);
                os_log::write_str_raw(" cr3=0x");
                os_log::write_u64_hex_raw(cr3);
                os_log::write_str_raw(" PML4[256]=0x");
                os_log::write_u64_hex_raw(current_entry);
                os_log::write_str_raw(" expected=0x");
                os_log::write_u64_hex_raw(golden);
                os_log::write_str_raw("\n");

                // Check if PML4 frame itself is a freed buddy block
                let pml4_first = core::ptr::read_volatile(pml4_virt.as_ptr::<u64>());
                if pml4_first == 0x4652454542304C {
                    os_log::write_str_raw("[PML4-LIVE-CORRUPT] PML4 IS FreeBlock! USE-AFTER-FREE!\n");
                }
            }
        }
    }

    // Check sleep queue and wake any tasks whose sleep time has expired
    // — GraveShift: BSP-only. All tasks live on CPU 0 (no migration yet).
    // Running on APs would redundantly scan + send cross-CPU IPIs.
    if sched::this_cpu() == 0 {
        syscall::time::check_sleepers();
    }

    // Debug: full scheduler dump every ~3 seconds (300 ticks)
    // — GraveShift: BSP-only. APs have empty run queues (no task
    // migration yet), so their dumps are blank noise. Running on
    // one CPU also prevents 4 dumps interleaving on serial.
    #[cfg(feature = "debug-sched")]
    {
        let ticks = arch::timer_ticks();
        if ticks % 300 == 0 && sched::this_cpu() == 0 {
            if let Some((curr_pid, min_vr, tasks)) = sched::debug_dump_all() {
                unsafe {
                    use arch_x86_64::serial::{write_byte_unsafe, write_str_unsafe};

                    // Helper: print u64 decimal (interrupt-safe, no alloc)
                    fn print_u64(n: u64) {
                        unsafe {
                            if n == 0 {
                                write_byte_unsafe(b'0');
                                return;
                            }
                            let mut buf = [0u8; 20];
                            let mut v = n;
                            let mut pos = 0;
                            while v > 0 {
                                buf[pos] = b'0' + (v % 10) as u8;
                                v /= 10;
                                pos += 1;
                            }
                            for i in (0..pos).rev() {
                                write_byte_unsafe(buf[i]);
                            }
                        }
                    }
                    fn print_i8(n: i8) {
                        unsafe {
                            if n < 0 {
                                write_byte_unsafe(b'-');
                                print_u64((-n) as u64);
                            } else {
                                print_u64(n as u64);
                            }
                        }
                    }

                    write_str_unsafe("\n===== SCHED DUMP t=");
                    print_u64(ticks);
                    write_str_unsafe(" curr=");
                    print_u64(curr_pid.unwrap_or(99) as u64);
                    write_str_unsafe(" min_vr=");
                    print_u64(min_vr / 1_000_000); // ms
                    write_str_unsafe("ms =====\n");
                    write_str_unsafe(
                        "PID PPID NAME         STATE       ON_RQ NICE VRUNTIME(ms)  RUNTIME(ms)  WAIT\n",
                    );
                    write_str_unsafe(
                        "--- ---- ------------ ----------- ----- ---- ------------- ------------ ----\n",
                    );

                    for t in &tasks {
                        // PID (3 chars)
                        if t.pid < 10 {
                            write_str_unsafe("  ");
                        } else if t.pid < 100 {
                            write_byte_unsafe(b' ');
                        }
                        print_u64(t.pid as u64);
                        write_byte_unsafe(b' ');

                        // PPID (4 chars)
                        if t.ppid < 10 {
                            write_str_unsafe("   ");
                        } else if t.ppid < 100 {
                            write_str_unsafe("  ");
                        } else if t.ppid < 1000 {
                            write_byte_unsafe(b' ');
                        }
                        print_u64(t.ppid as u64);
                        write_byte_unsafe(b' ');

                        // NAME (12 chars)
                        if t.name_len > 0 {
                            for i in 0..t.name_len.min(12) {
                                write_byte_unsafe(t.name[i]);
                            }
                            for _ in t.name_len.min(12)..12 {
                                write_byte_unsafe(b' ');
                            }
                        } else {
                            write_str_unsafe("???         ");
                        }
                        write_byte_unsafe(b' ');

                        // STATE (11 chars)
                        let state_str = match t.state {
                            TaskState::TASK_RUNNING => "RUNNING    ",
                            TaskState::TASK_INTERRUPTIBLE => "SLEEPING   ",
                            TaskState::TASK_UNINTERRUPTIBLE => "DISK_SLEEP ",
                            TaskState::TASK_ZOMBIE => "ZOMBIE     ",
                            TaskState::TASK_STOPPED => "STOPPED    ",
                            _ => "UNKNOWN    ",
                        };
                        write_str_unsafe(state_str);
                        write_byte_unsafe(b' ');

                        // ON_RQ
                        if t.on_rq {
                            write_str_unsafe("yes  ");
                        } else {
                            write_str_unsafe("no   ");
                        }
                        write_byte_unsafe(b' ');

                        // NICE (4 chars)
                        if t.nice >= 0 {
                            write_byte_unsafe(b' ');
                        }
                        print_i8(t.nice);
                        if t.nice > -10 && t.nice < 10 {
                            write_byte_unsafe(b' ');
                        }
                        write_byte_unsafe(b' ');

                        // VRUNTIME in ms (13 chars)
                        let vr_ms = t.vruntime / 1_000_000;
                        print_u64(vr_ms);
                        write_str_unsafe("ms");
                        // Padding
                        let mut vr_digits = if vr_ms == 0 { 1 } else { 0 };
                        {
                            let mut v = vr_ms;
                            while v > 0 {
                                vr_digits += 1;
                                v /= 10;
                            }
                        }
                        for _ in 0..10usize.saturating_sub(vr_digits + 2) {
                            write_byte_unsafe(b' ');
                        }
                        write_byte_unsafe(b' ');

                        // RUNTIME in ms (12 chars)
                        let rt_ms = t.sum_exec_runtime / 1_000_000;
                        print_u64(rt_ms);
                        write_str_unsafe("ms");
                        let mut rt_digits = if rt_ms == 0 { 1 } else { 0 };
                        {
                            let mut v = rt_ms;
                            while v > 0 {
                                rt_digits += 1;
                                v /= 10;
                            }
                        }
                        for _ in 0..9usize.saturating_sub(rt_digits + 2) {
                            write_byte_unsafe(b' ');
                        }
                        write_byte_unsafe(b' ');

                        // WAIT
                        if t.waiting_for_child != 0 {
                            write_str_unsafe("w=");
                            if t.waiting_for_child < 0 {
                                write_str_unsafe("any");
                            } else {
                                print_u64(t.waiting_for_child as u64);
                            }
                        } else {
                            write_byte_unsafe(b'-');
                        }
                        write_byte_unsafe(b'\n');
                    }
                    write_str_unsafe("==========\n\n");
                }
            }
        }
    }

    let frame = unsafe { &mut *(current_rsp as *mut InterruptFrame) };

    // Use lock-free current PID to avoid deadlock in interrupt context
    let current_pid = sched::current_pid_lockfree().unwrap_or(0);

    // Signal delivery: when returning to user mode, check for pending signals
    // with default Terminate/CoreDump action and kill the process.
    // This is how Ctrl+C (SIGINT) actually terminates running programs.
    let in_user_mode = frame.cs == 0x23;
    if in_user_mode && current_pid > 1 {
        if let Some(meta_arc) = sched::get_task_meta(current_pid) {
            match meta_arc.try_lock() {
            Some(mut meta) => {
                if meta.has_pending_signals() {
                    // — GraveShift: ISR signal path trace — confirm we actually see pending signals
                    unsafe {
                        os_log::write_str_raw("[ISR-SIG] p=");
                        let pid_b = current_pid as u8;
                        if pid_b >= 10 { os_log::write_byte_raw(b'0' + (pid_b / 10)); }
                        os_log::write_byte_raw(b'0' + (pid_b % 10));
                        os_log::write_str_raw(" HAS pending\n");
                    }
                    let signal_mask = meta.signal_mask;
                    if let Some(pending) = meta.pending_signals.dequeue(&signal_mask) {
                        let signo = pending.signo;
                        let action = if signo >= 1 && signo <= signal::NSIG as i32 {
                            meta.sigactions[(signo - 1) as usize]
                        } else {
                            signal::SigAction::new()
                        };
                        let result = determine_action(&pending, &action, &signal_mask);

                        match result {
                            SignalResult::Terminate | SignalResult::CoreDump => {
                                // — GraveShift: ISR Terminate trace — confirm we reach this path
                                unsafe {
                                    os_log::write_str_raw("[ISR-SIG] TERMINATE p=");
                                    let pid_b = current_pid as u8;
                                    if pid_b >= 10 { os_log::write_byte_raw(b'0' + (pid_b / 10)); }
                                    os_log::write_byte_raw(b'0' + (pid_b % 10));
                                    os_log::write_str_raw(" sig=");
                                    os_log::write_byte_raw(b'0' + signo as u8);
                                    os_log::write_str_raw("\n");
                                }
                                // Release ProcessMeta lock before calling scheduler functions
                                drop(meta);

                                // — GraveShift: Linux waitpid format for signal death:
                                // bits [6:0] = signal number, bit 7 = core dump (0 here).
                                // WTERMSIG(status) = status & 0x7F. Raw "128 + signo" was
                                // wrong — made WIFEXITED true and WEXITSTATUS = 128+sig.
                                let exit_status = signo & 0x7F;
                                sched::set_task_exit_status(current_pid, exit_status);

                                // — GraveShift: Wake parent so it can reap via wait().
                                // MUST use try_wake_up — we're in ISR context. Blocking
                                // wake_up() would deadlock if the parent's CPU holds its
                                // RQ lock. If try_wake_up fails (contention), the parent
                                // wakes on its next timer tick or scheduler_tick anyway.
                                if let Some(ppid) = sched::get_task_ppid(current_pid) {
                                    if ppid > 0 {
                                        sched::try_wake_up(ppid);
                                    }
                                }

                                // Force context switch away from this zombie
                                sched::set_need_resched();
                            }
                            SignalResult::UserHandler {
                                handler,
                                signo,
                                info,
                                flags: _,
                                handler_mask,
                            } => {
                                // 🔥 GraveShift: User signal handlers - the UNIX way 🔥
                                // Setup signal frame on user stack and redirect execution

                                // Extract saved registers from interrupt frame
                                let regs = signal::delivery::SavedRegisters {
                                    rax: frame.rax,
                                    rbx: frame.rbx,
                                    rcx: frame.rcx,
                                    rdx: frame.rdx,
                                    rsi: frame.rsi,
                                    rdi: frame.rdi,
                                    rbp: frame.rbp,
                                    r8: frame.r8,
                                    r9: frame.r9,
                                    r10: frame.r10,
                                    r11: frame.r11,
                                    r12: frame.r12,
                                    r13: frame.r13,
                                    r14: frame.r14,
                                    r15: frame.r15,
                                };

                                // Get signal restorer (sigreturn trampoline) from action
                                let restorer = action.sa_restorer;

                                // Setup signal frame
                                let (new_rip, new_rsp, sig_frame) =
                                    signal::delivery::setup_signal_handler(
                                        handler,
                                        signo,
                                        info,
                                        action.flags(),
                                        restorer,
                                        meta.signal_mask,
                                        frame.rip,
                                        frame.rsp,
                                        frame.rflags,
                                        &regs,
                                    );

                                // Write signal frame to user stack
                                // 🔥 GraveShift: Direct write - page fault will catch invalid stack 🔥
                                let frame_ptr = new_rsp as *mut signal::delivery::SignalFrame;
                                unsafe {
                                    core::ptr::write(frame_ptr, sig_frame);
                                }

                                // Update process signal mask for handler execution
                                meta.signal_mask = handler_mask;

                                // Redirect execution to signal handler
                                frame.rip = new_rip;
                                frame.rsp = new_rsp;
                                frame.rdi = signo as u64; // First arg: signal number
                            }
                            SignalResult::Ignore => {
                                // Signal ignored - do nothing
                            }
                            SignalResult::Stop => {
                                // — ThreadRogue: freezing the process in its tracks.
                                // ISR context — all wakes MUST be try_wake_up to avoid
                                // deadlock on contended RQ locks. block_current uses
                                // with_rq but is safe here because user-mode code
                                // (cs==0x23 gate above) never holds kernel locks.
                                meta.stop_signal = Some(signo as u8);
                                meta.continued = false;

                                let ppid_opt = sched::get_task_ppid(current_pid);
                                drop(meta);

                                sched::block_current(TaskState::TASK_STOPPED);

                                if let Some(ppid) = ppid_opt {
                                    if ppid > 0 {
                                        sched::try_wake_up(ppid);
                                    }
                                }
                            }
                            SignalResult::Continue => {
                                // — ThreadRogue: thawing from the ice.
                                // ISR context — try_wake_up only. If contention
                                // prevents wake, next timer tick catches it.
                                meta.stop_signal = None;
                                meta.continued = true;

                                let ppid_opt = sched::get_task_ppid(current_pid);
                                drop(meta);

                                // If task was stopped, wake it back up
                                if sched::get_task_state(current_pid)
                                    == Some(TaskState::TASK_STOPPED)
                                {
                                    sched::try_wake_up(current_pid);
                                }

                                // Wake parent for WCONTINUED waitpid
                                if let Some(ppid) = ppid_opt {
                                    if ppid > 0 {
                                        sched::try_wake_up(ppid);
                                    }
                                }
                            }
                            SignalResult::None => {
                                // No signal - shouldn't happen here
                            }
                        }
                    }
                }
            } // — end Some(mut meta) arm
            None => {
                // — GraveShift: ISR try_lock failure — meta is contended during timer tick
                // This means someone else holds ProcessMeta. Signal delivery deferred.
            }
            } // — end match meta_arc.try_lock()
        }
    }

    // — GraveShift: Linux-model preempt_count check. preemptable() returns true
    // when count == 0 (no locks held). KernelMutex increments on lock, decrements
    // on unlock. Old kpo call sites also work through the backward-compat aliases.
    let kernel_preempt_ok = arch::preemptable();
    let preempt_count_val = arch::get_preempt_count();

    // Only preempt:
    // - User mode (CS = 0x23) - always safe
    // - Kernel mode if KERNEL_PREEMPT_OK flag is set (blocking syscalls)
    let in_kernel = frame.cs != 0x23;

    // — CrashBloom: Kernel-mode stall watchdog. If the same task has been stuck in
    // kernel mode for 200+ ticks (2 seconds) without a context switch, dump the RIP
    // so we can see WHERE it's spinning. Nondeterministic hangs need observability.
    {
        use core::sync::atomic::{AtomicU32, AtomicU64, Ordering as AO};
        static STALL_PID: AtomicU32 = AtomicU32::new(0);
        static STALL_COUNT: AtomicU32 = AtomicU32::new(0);
        static STALL_REPORTED: AtomicU32 = AtomicU32::new(0);

        if in_kernel && current_pid > 1 {
            let prev = STALL_PID.load(AO::Relaxed);
            if prev == current_pid {
                let count = STALL_COUNT.fetch_add(1, AO::Relaxed) + 1;
                // Report every 200 ticks (2 sec) while stalled
                if count % 200 == 0 && STALL_REPORTED.load(AO::Relaxed) < 5 {
                    STALL_REPORTED.fetch_add(1, AO::Relaxed);
                    unsafe {
                        os_log::write_str_raw("[STALL] pid=");
                        os_log::write_u32_raw(current_pid);
                        os_log::write_str_raw(" kernel-mode ");
                        os_log::write_u32_raw(count);
                        os_log::write_str_raw(" ticks, RIP=0x");
                        os_log::write_u64_hex_raw(frame.rip);
                        os_log::write_str_raw(" RSP=0x");
                        os_log::write_u64_hex_raw(frame.rsp);
                        os_log::write_str_raw(" preempt_count=");
                        os_log::write_u32_raw(preempt_count_val as u32);
                        os_log::write_str_raw("\n");
                    }
                }
            } else {
                STALL_PID.store(current_pid, AO::Relaxed);
                STALL_COUNT.store(1, AO::Relaxed);
                STALL_REPORTED.store(0, AO::Relaxed);
            }
        } else {
            // Reset if user mode or idle
            STALL_COUNT.store(0, AO::Relaxed);
            STALL_REPORTED.store(0, AO::Relaxed);
        }
    }

    if in_kernel && !kernel_preempt_ok {
        // — GraveShift: Linux model — don't preempt kernel mode without kpo.
        // spin_lock() holders don't set kpo. If we preempt them, the next task
        // tries the same lock → permanent deadlock. The task will finish its
        // syscall, return to userspace, and get preempted there.
        //
        // Safety net: if a task has been in kernel mode without kpo for 500+
        // ticks (5 seconds), something is genuinely stuck (not a spinlock — no
        // spinlock is held for 5 seconds). Force-preempt to prevent permanent
        // CPU lockup. This is purely a recovery mechanism for pathological cases
        // like a driver polling loop that forgot kpo.
        use core::sync::atomic::{AtomicU32, AtomicU64, Ordering as AO};

        // — GraveShift: 50 ticks = 500ms. No spinlock is held for 500ms.
        // The longest legitimate non-kpo path is <1ms (lock acquire, PT walk).
        // virtio-blk polling takes 10-50ms typically — this catches stuck drivers
        // without making the whole system crawl at 5 seconds per context switch.
        const EMERGENCY_TICKS: u32 = 50; // 500ms — no spinlock lasts this long
        const MAX_CPUS: usize = 8;
        static NOKPO_PID: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];
        static NOKPO_COUNT: [AtomicU32; MAX_CPUS] = [const { AtomicU32::new(0) }; MAX_CPUS];

        let cpu = sched::this_cpu() as usize;
        let cpu_idx = if cpu < MAX_CPUS { cpu } else { 0 };

        let pid64 = current_pid as u64;
        let prev_pid = NOKPO_PID[cpu_idx].load(AO::Relaxed);
        let streak = if prev_pid == pid64 {
            NOKPO_COUNT[cpu_idx].fetch_add(1, AO::Relaxed) + 1
        } else {
            NOKPO_PID[cpu_idx].store(pid64, AO::Relaxed);
            NOKPO_COUNT[cpu_idx].store(1, AO::Relaxed);
            1
        };

        // Still tick vruntime for CFS accounting
        sched::scheduler_tick();

        if streak < EMERGENCY_TICKS {
            return current_rsp;
        }

        // — CrashBloom: Emergency preempt — task stuck in kernel for 5+ seconds.
        // Reset streak so it gets a fresh window if re-scheduled.
        NOKPO_COUNT[cpu_idx].store(0, AO::Relaxed);
        // Fall through to context switch path
    } else {
        // — GraveShift: Normal path — task is in userspace or has kpo=1.
        // Tick the scheduler and check if preemption is needed.
        let need_resched = sched::scheduler_tick_ex(kernel_preempt_ok);

        if !need_resched && !sched::need_resched() {
            return current_rsp;
        }
    }

    // — SableWire: ISR lock safety gate. The context-switch path below calls
    // pick_next_task, set_task_context, get_task_switch_info — all use with_rq
    // (blocking lock). If the interrupted code was inside a scheduler operation
    // (yield_current, block_current, set_need_resched) the RQ lock is held.
    // Attempting with_rq here would deadlock the ISR forever. Bail and retry
    // on the next tick — the interrupted code will release the lock and complete.
    if !sched::rq_lock_available() {
        return current_rsp;
    }

    // Find next task to run using the scheduler
    let next_pid = pick_next_process(current_pid);

    if next_pid == current_pid {
        // Nothing to switch to — clear need_resched so we don't repeat the
        // expensive pick_next_task path on every tick for no reason.
        // A future wake_up() or block_current() will set it again when needed.
        return current_rsp;
    }

    // — PatchBay: Record context switch for performance monitoring
    perf::counters().record_context_switch();

    // Read FS base from MSR for TLS context preservation
    let current_fs_base: u64;
    unsafe {
        core::arch::asm!(
            "mov ecx, 0xC0000100",  // MSR IA32_FS_BASE
            "rdmsr",
            "shl rdx, 32",
            "or rax, rdx",
            out("rax") current_fs_base,
            out("rcx") _,
            out("rdx") _,
            options(nostack, preserves_flags)
        );
    }

    // — SableWire: Read user GS base from KERNEL_GS_BASE (0xC0000102).
    // In kernel context (after swapgs at ISR entry from user, or always for kernel ISR):
    //   GS_BASE       (0xC0000101) = kernel per-CPU data
    //   KERNEL_GS_BASE (0xC0000102) = user's saved GS value
    // We capture it here so context switches properly restore user GS on the way out.
    let current_gs_base: u64;
    unsafe {
        core::arch::asm!(
            "mov ecx, 0xC0000102",  // MSR IA32_KERNEL_GS_BASE
            "rdmsr",
            "shl rdx, 32",
            "or rax, rdx",
            out("rax") current_gs_base,
            out("rcx") _,
            out("rdx") _,
            options(nostack, preserves_flags)
        );
    }

    // Build current task's context from interrupt frame + MSR values
    let current_ctx = sched::TaskContext {
        rip: frame.rip,
        rsp: frame.rsp,
        rflags: frame.rflags,
        rax: frame.rax,
        rbx: frame.rbx,
        rcx: frame.rcx,
        rdx: frame.rdx,
        rsi: frame.rsi,
        rdi: frame.rdi,
        rbp: frame.rbp,
        r8: frame.r8,
        r9: frame.r9,
        r10: frame.r10,
        r11: frame.r11,
        r12: frame.r12,
        r13: frame.r13,
        r14: frame.r14,
        r15: frame.r15,
        cs: frame.cs,
        ss: frame.ss,
        fs_base: current_fs_base,
        gs_base: current_gs_base,
    };

    // — TorqueJax: One lock to rule them all.
    // context_switch_transaction replaces five separate RQ lock acquisitions:
    //   1. save_kernel_preempt  (outgoing task's kpo flag)
    //   2. set_task_context     (save interrupt frame → Task.context)
    //   3. get_task_switch_info (read incoming CR3/stack/ctx)
    //   4. switch_to            (re-enqueue old, dequeue new, set rq.curr)
    //   5. load_kernel_preempt  (incoming task's saved kpo flag)
    // All five happen under a single try_with_rq. Less contention, no state
    // drift between operations, one cache miss instead of five.
    // — GraveShift: Pass the raw preempt_count to the context switch transaction.
    // The outgoing task's count is saved in its Task struct so it resumes with the
    // correct lock depth. The incoming task's saved count is restored below.
    let switch_info = match sched::context_switch_transaction(
        current_pid,
        next_pid,
        current_ctx,
        preempt_count_val,
    ) {
        Some(info) => info,
        None => return current_rsp, // Lock contended or task not found — retry next tick
    };

    // — GraveShift: Clear the outgoing task's preempt_count on this CPU.
    // We saved it in the task struct; it'll be restored when this task is
    // switched back in. The incoming task's count is set below.
    arch::set_preempt_count(0);

    let next_ctx = switch_info.new_ctx;
    let kernel_stack_top = {
        let ks_virt = phys_to_virt(switch_info.new_kernel_stack);
        ks_virt.as_u64() + switch_info.new_kernel_stack_size as u64
    };

    // Debug: log context switches with process names (interrupt-safe)
    #[cfg(feature = "debug-sched")]
    unsafe {
        use arch_x86_64::serial::{write_byte_unsafe, write_str_unsafe};
        fn print_pid(pid: u32) {
            unsafe {
                if pid == 0 {
                    write_byte_unsafe(b'0');
                    return;
                }
                let mut buf = [0u8; 10];
                let mut n = pid;
                let mut pos = 0;
                while n > 0 {
                    buf[pos] = b'0' + (n % 10) as u8;
                    n /= 10;
                    pos += 1;
                }
                for i in (0..pos).rev() {
                    write_byte_unsafe(buf[i]);
                }
            }
        }
        fn print_task_name(pid: u32) {
            if pid == 0 {
                unsafe {
                    write_str_unsafe("idle");
                }
                return;
            }
            if let Some(meta_arc) = sched::get_task_meta(pid) {
                if let Some(meta) = meta_arc.try_lock() {
                    if let Some(cmd) = meta.cmdline.first() {
                        let bytes = cmd.as_bytes();
                        let start = bytes
                            .iter()
                            .rposition(|&b| b == b'/')
                            .map(|i| i + 1)
                            .unwrap_or(0);
                        let len = (bytes.len() - start).min(16);
                        for i in 0..len {
                            unsafe {
                                write_byte_unsafe(bytes[start + i]);
                            }
                        }
                        return;
                    }
                }
            }
            unsafe {
                write_str_unsafe("???");
            }
        }
        write_str_unsafe("[SWITCH] ");
        print_pid(current_pid);
        write_str_unsafe("(");
        print_task_name(current_pid);
        write_str_unsafe(")->");
        print_pid(next_pid);
        write_str_unsafe("(");
        print_task_name(next_pid);
        write_str_unsafe(") cs=");
        if frame.cs == 0x23 {
            write_str_unsafe("user");
        } else {
            write_str_unsafe("kern");
        }
        write_str_unsafe("\n");
    }

    // ALWAYS update kernel stack pointers when switching tasks.
    // Even if the next task is currently in kernel mode (CS=0x08, preempted during
    // a syscall), it will eventually return to user mode and may make new syscalls.
    // The syscall entry point reads kernel RSP from GS:[0] (CPU_DATA), so it must
    // always reflect the CURRENT task's kernel stack. Similarly, TSS.RSP0 must be
    // correct for interrupts that trigger privilege-level changes.
    unsafe {
        arch::syscall::set_kernel_stack(kernel_stack_top);
    }
    arch::gdt::set_kernel_stack(kernel_stack_top);

    // — CrashBloom: PRE-SWITCH PML4[256] CANARY CHECK.
    // Validate the target CR3's kernel entries before loading it. If PML4[256]
    // (direct physical map) is corrupted, switching to this CR3 would immediately
    // triple-fault because even the page fault handler lives in kernel space.
    // One memory read to prevent a silent, undiagnosable death. Worth every cycle.
    unsafe {
        let golden = crate::globals::KERNEL_PML4_256_ENTRY;
        if golden != 0 {
            let target_pml4_virt = mm_paging::phys_to_virt(PhysAddr::new(switch_info.new_cr3));
            let target_entry = core::ptr::read_volatile(
                target_pml4_virt.as_ptr::<u64>().add(256)
            );
            if target_entry != golden {
                // — CrashBloom: CORRUPTION DETECTED! Log everything and skip the switch.
                // Without this check we'd triple-fault with zero diagnostic output.
                os_log::write_str_raw("[PML4-CORRUPT] pid=");
                os_log::write_u32_raw(next_pid);
                os_log::write_str_raw(" cr3=0x");
                os_log::write_u64_hex_raw(switch_info.new_cr3);
                os_log::write_str_raw(" PML4[256]=0x");
                os_log::write_u64_hex_raw(target_entry);
                os_log::write_str_raw(" expected=0x");
                os_log::write_u64_hex_raw(golden);
                os_log::write_str_raw(" — SKIPPING SWITCH, killing task\n");

                // Also check PML4 frame itself — is it a freed buddy block?
                let pml4_first = core::ptr::read_volatile(
                    target_pml4_virt.as_ptr::<u64>()
                );
                if pml4_first == 0x4652454542304C {
                    os_log::write_str_raw("[PML4-CORRUPT] PML4 frame IS a FreeBlock! USE-AFTER-FREE!\n");
                }

                // Kill the corrupted task so we don't keep trying to switch to it
                sched::set_task_exit_status(next_pid, 139); // SIGSEGV-style exit
                sched::set_need_resched();
                return current_rsp;
            }
        }
    }

    // Switch page tables
    unsafe {
        core::arch::asm!("mov cr3, {}", in(reg) switch_info.new_cr3);
    }

    // Restore FS base MSR for next process (TLS support)
    if next_ctx.fs_base != 0 {
        unsafe {
            core::arch::asm!(
                "mov ecx, 0xC0000100",  // MSR IA32_FS_BASE
                "mov rax, {fs_base}",
                "mov rdx, {fs_base}",
                "shr rdx, 32",
                "wrmsr",
                fs_base = in(reg) next_ctx.fs_base,
                out("rax") _,
                out("rcx") _,
                out("rdx") _,
                options(nostack, preserves_flags)
            );
        }
    }

    // — SableWire: Restore incoming task's user GS base to KERNEL_GS_BASE (0xC0000102).
    // The outgoing task's user GS was saved (line ~660, rdmsr 0xC0000102) into its
    // TaskContext.gs_base. Without restoring the incoming task's value here, the ISR
    // exit swapgs gives the new task the OLD task's user GS. For TLS-heavy workloads
    // (Go, Rust std) this silently corrupts thread-local storage across context switches.
    unsafe {
        core::arch::asm!(
            "mov ecx, 0xC0000102",  // MSR IA32_KERNEL_GS_BASE
            "mov rax, {gs_base}",
            "mov rdx, {gs_base}",
            "shr rdx, 32",
            "wrmsr",
            gs_base = in(reg) next_ctx.gs_base,
            out("rax") _,
            out("rcx") _,
            out("rdx") _,
            options(nostack, preserves_flags)
        );
    }

    // — GraveShift: Restore the incoming task's preempt_count. Without this, a task
    // preempted while holding a KernelMutex (count > 0) would resume with count=0,
    // making the scheduler think it's safe to preempt again mid-lock. The saved count
    // preserves the exact lock nesting depth across context switches.
    arch::set_preempt_count(switch_info.new_preempt_count);

    // Build interrupt frame for next process.
    //
    // CRITICAL: For kernel-mode tasks (CS=0x08), we must NOT place the frame
    // at kernel_stack_top because that would overwrite the syscall entry's saved
    // user context. The task was preempted while in kernel code (e.g., kernel_wait
    // or kernel_yield), and the syscall handler's saved user registers live at the
    // top of the kernel stack. Overwriting them causes corruption when the task
    // eventually returns to user mode via sysretq.
    //
    // For kernel-mode tasks: place the frame below the task's saved RSP. The space
    // there was the original interrupt frame (already consumed) and is safe to reuse.
    //
    // For user-mode tasks (CS=0x23): kernel_stack_top is correct because the user
    // task doesn't have live data on the kernel stack.
    // — BlackLatch: CRITICAL FIX — resolve CS first, then derive SS from it.
    // In x86-64 long mode, interrupt delivery from ring 3→0 sets SS=0 (null selector).
    // Task contexts can end up with ss=0 through scheduler state corruption (e.g., AP
    // idle tasks without real Task structs, or edge cases during context_switch_transaction
    // failures). The old code defaulted ALL zero-SS contexts to USER_DATA (0x1B). For
    // kernel-mode returns (CS=0x08), iretq requires SS.RPL == CPL(0). SS=0x1B has RPL=3
    // → #GP(0x18). Only two valid combos exist in our GDT:
    //   CS=0x08 (kernel) → SS=0x10 (KERNEL_DATA)
    //   CS=0x23 (user)   → SS=0x1B (USER_DATA)
    // Deriving SS from CS prevents the GPF regardless of context corruption. — BlackLatch
    let cs = if next_ctx.cs != 0 { next_ctx.cs } else { 0x23 };
    let ss = if cs == 0x08 { 0x10 } else { 0x1B };
    let is_kernel_mode = cs == 0x08;

    // — BlackLatch: Diagnostic — catch any context that would have GPF'd before the fix.
    // If the saved SS doesn't match what CS demands, log it so we can trace the corruption.
    #[cfg(feature = "debug-sched")]
    if next_ctx.cs != 0 && next_ctx.ss != ss {
        unsafe {
            use arch_x86_64::serial::{write_byte_unsafe, write_str_unsafe};
            write_str_unsafe("[SCHED-GPF-GUARD] ctx.cs=0x");
            let cs_hi = ((next_ctx.cs >> 4) & 0xF) as u8;
            let cs_lo = (next_ctx.cs & 0xF) as u8;
            write_byte_unsafe(if cs_hi < 10 { b'0' + cs_hi } else { b'a' + cs_hi - 10 });
            write_byte_unsafe(if cs_lo < 10 { b'0' + cs_lo } else { b'a' + cs_lo - 10 });
            write_str_unsafe(" ctx.ss=0x");
            let ss_hi = ((next_ctx.ss >> 4) & 0xF) as u8;
            let ss_lo = (next_ctx.ss & 0xF) as u8;
            write_byte_unsafe(if ss_hi < 10 { b'0' + ss_hi } else { b'a' + ss_hi - 10 });
            write_byte_unsafe(if ss_lo < 10 { b'0' + ss_lo } else { b'a' + ss_lo - 10 });
            write_str_unsafe(" fixed_ss=0x");
            let fss_hi = ((ss >> 4) & 0xF) as u8;
            let fss_lo = (ss & 0xF) as u8;
            write_byte_unsafe(if fss_hi < 10 { b'0' + fss_hi } else { b'a' + fss_hi - 10 });
            write_byte_unsafe(if fss_lo < 10 { b'0' + fss_lo } else { b'a' + fss_lo - 10 });
            write_str_unsafe(" pid=");
            let pid_b = next_pid as u8;
            if pid_b >= 100 { write_byte_unsafe(b'0' + (pid_b / 100)); }
            if pid_b >= 10 { write_byte_unsafe(b'0' + ((pid_b / 10) % 10)); }
            write_byte_unsafe(b'0' + (pid_b % 10));
            write_str_unsafe("\n");
        }
    }

    let frame_size = core::mem::size_of::<InterruptFrame>() as u64;
    let raw_ptr = if is_kernel_mode {
        // Place below the task's saved kernel RSP (where original interrupt frame was)
        next_ctx.rsp - frame_size
    } else {
        // User-mode: top of kernel stack is safe
        kernel_stack_top - frame_size
    };
    // — GraveShift: align down to 8-byte boundary. A misaligned RSP from
    // corrupted saved context must not panic the scheduler — align it and
    // continue. The task may crash in user mode but the kernel survives.
    let new_frame_ptr = (raw_ptr & !7u64) as *mut InterruptFrame;

    unsafe {

        (*new_frame_ptr).ss = ss;
        (*new_frame_ptr).rsp = next_ctx.rsp;
        (*new_frame_ptr).rflags = next_ctx.rflags | 0x200; // Ensure IF set
        (*new_frame_ptr).cs = cs;
        (*new_frame_ptr).rip = next_ctx.rip;
        (*new_frame_ptr).rax = next_ctx.rax;
        (*new_frame_ptr).rbx = next_ctx.rbx;
        (*new_frame_ptr).rcx = next_ctx.rcx;
        (*new_frame_ptr).rdx = next_ctx.rdx;
        (*new_frame_ptr).rsi = next_ctx.rsi;
        (*new_frame_ptr).rdi = next_ctx.rdi;
        (*new_frame_ptr).rbp = next_ctx.rbp;
        (*new_frame_ptr).r8 = next_ctx.r8;
        (*new_frame_ptr).r9 = next_ctx.r9;
        (*new_frame_ptr).r10 = next_ctx.r10;
        (*new_frame_ptr).r11 = next_ctx.r11;
        (*new_frame_ptr).r12 = next_ctx.r12;
        (*new_frame_ptr).r13 = next_ctx.r13;
        (*new_frame_ptr).r14 = next_ctx.r14;
        (*new_frame_ptr).r15 = next_ctx.r15;
    }

    new_frame_ptr as u64
}

/// Pick the next process to run
///
/// Uses the sched crate's scheduling policy (RT > Fair/CFS > Idle).
/// CFS picks the task with lowest vruntime, which naturally favors:
/// - Just-woken tasks (blocked tasks don't accumulate vruntime)
/// - Tasks that haven't run recently
fn pick_next_process(current_pid: u32) -> u32 {
    // — GraveShift: pick_next_task() mutates scheduler state (pops from CFS
    // tree, changes rq.curr, clears need_resched). DO NOT validate or reject
    // the result — that would leave the scheduler in a corrupted state where
    // rq.curr points to one task but the CPU is running another.
    if let Some(next_pid) = sched::pick_next_task() {
        return next_pid;
    }

    // — GraveShift: Fallback — only reached when with_rq returns None (lock
    // contention). NEVER fall back to a zombie — that creates an unkillable
    // revenant that resumes user code after signal termination. Return idle
    // (PID 0) if current is dead. The next timer tick retries properly.
    if let Some(state) = sched::get_task_state(current_pid) {
        if state == TaskState::TASK_ZOMBIE || state == TaskState::TASK_DEAD {
            return 0; // idle
        }
    }
    current_pid
}

/// Wake up a process that was blocked
///
/// Called when a blocked process should become runnable.
pub fn wake_up(pid: u32) {
    #[cfg(feature = "debug-sched")]
    {
        crate::debug_sched!("[WAKE] pid={}", pid);
    }
    // Clear waiting state and wake via scheduler
    sched::clear_task_waiting(pid);
    sched::wake_up(pid);
}

/// Wake up a process that was blocked waiting for a child
///
/// Called from user_exit when a child process exits.
pub fn wake_parent(parent_pid: u32) {
    #[cfg(feature = "debug-sched")]
    {
        crate::debug_sched!("[WAKE-PARENT] pid={}", parent_pid);
    }
    wake_up(parent_pid);
}

/// Block the current process
///
/// Sets the task to blocked state via scheduler.
pub fn block_current(state: TaskState) {
    #[cfg(feature = "debug-sched")]
    {
        let pid = sched::current_pid().unwrap_or(0);
        crate::debug_sched!("[BLOCK] pid={} state={:?}", pid, state);
    }
    sched::block_current(state);
}

/// Voluntary yield - called from sched_yield syscall
///
/// Signals the scheduler that this task wants to yield, then lets the
/// timer interrupt handle the actual context switch via iretq.
/// This is safe because iretq correctly restores both user-mode and
/// kernel-mode contexts (unlike sysretq which always returns to Ring 3).
/// Returns 0 on success.
pub fn kernel_yield() -> i64 {
    // Tell sched crate we're yielding (updates vruntime accounting)
    sched::yield_current();

    // Request a reschedule - the next timer interrupt will perform the
    // actual context switch via scheduler_tick + iretq
    sched::set_need_resched();

    // Allow kernel preemption so the timer interrupt can switch us out
    arch::allow_kernel_preempt();

    // Brief halt to give the timer a chance to fire and switch us.
    // sti + hlt must be in the same asm block so an interrupt can't
    // fire between them and cause an extra tick of delay.
    unsafe {
        core::arch::asm!("sti", "hlt", options(nomem, nostack));
    }

    // If we get here, the timer fired and the scheduler chose to keep us running
    arch::disallow_kernel_preempt();

    0
}

/// Set a process's scheduling policy
pub fn set_scheduler(pid: u32, policy: SchedPolicy, priority: u8) {
    sched::set_scheduler(pid, policy, priority);
}

/// Get a process's scheduling policy
pub fn get_scheduler(pid: u32) -> Option<(SchedPolicy, u8)> {
    sched::get_scheduler(pid)
}

/// Set a task's nice value
pub fn set_nice(pid: u32, nice: i8) {
    sched::set_nice(pid, nice);
}

/// Get a process's nice value
pub fn get_nice(pid: u32) -> Option<i8> {
    sched::get_nice(pid)
}

/// Set CPU affinity for a process
pub fn set_affinity(pid: u32, cpuset: sched::CpuSet) {
    sched::set_affinity(pid, cpuset);
}

/// Get CPU affinity for a process
pub fn get_affinity(pid: u32) -> Option<sched::CpuSet> {
    sched::get_affinity(pid)
}

/// Check for pending signals on syscall return
///
/// Called by the syscall dispatch mechanism just before returning to userspace.
/// -- BlackLatch: Terminate the current user process after a fatal hardware fault.
/// Called from exception handlers (page fault, divide error, etc.) when a
/// user-mode instruction triggers an unrecoverable fault. The process becomes
/// a zombie and the scheduler switches to the next runnable task.
///
/// Arguments match UserFaultKillCallback: (pid_hint, faulting_rip, signal_number)
/// pid_hint == 0 means "current process".
pub fn kill_faulting_process(_pid: u64, rip: u64, signo: u64) {
    let current_pid = match sched::current_pid() {
        Some(pid) if pid > 1 => pid,
        _ => return, // -- BlackLatch: Never kill idle or init
    };

    crate::debug_to_buffer!(
        "[KILL] PID {} terminated by signal {} at RIP {:#x}",
        current_pid,
        signo,
        rip
    );

    // — GraveShift: Close all file descriptors BEFORE zombie state.
    // Same bug as user_exit() — a signal-killed process holding PipeWrite keeps
    // has_writers()=true → parent's pipe read blocks forever → deadlock.
    // Zombies only need PID + exit status for waitpid. Kill the fds.
    if let Some(meta_arc) = sched::get_task_meta(current_pid) {
        let mut meta = meta_arc.lock();
        meta.fd_table = vfs::FdTable::new();
        meta.shared_fd_table = None;
    }

    // — GraveShift: Linux waitpid format for signal kill:
    // bits [6:0] = signal number. WTERMSIG(status) = status & 0x7F.
    let exit_status = (signo as i32) & 0x7F;
    sched::set_task_exit_status(current_pid, exit_status);

    // -- GraveShift: Wake parent so it can reap the corpse
    if let Some(ppid) = sched::get_task_ppid(current_pid) {
        if ppid > 0 {
            wake_up(ppid);
        }
    }

    // -- BlackLatch: Mark as zombie, force reschedule, then halt until
    // the scheduler pulls us off the CPU
    sched::block_current(TaskState::TASK_ZOMBIE);
    sched::set_need_resched();

    loop {
        unsafe {
            core::arch::asm!("sti", "hlt", options(nomem, nostack));
        }
    }
}

/// Checks for deliverable signals and modifies the syscall return context
/// to redirect to signal handlers if needed.
///
/// -- GraveShift: Signal delivery on syscall return, not just timer ticks.
/// Also handles deferred rescheduling — the timer ISR skips context switches
/// for kernel-mode tasks (CS=0x08), so a process hammering syscalls
/// (write/fork/open in a loop) monopolises the CPU. Checking need_resched
/// here — right before sysretq — gives the scheduler its only reliable
/// chance to preempt such tasks.
pub fn check_signals_on_syscall_return() {
    // — GraveShift: Sigreturn deferred restoration. sys_sigreturn() stashed a SignalFrame
    // because the asm resave clobbers SYSCALL_USER_CONTEXT after the handler returns.
    // Apply it here (under CLI, after resave) so it sticks through to sysretq.
    if let Some(frame) = signal::delivery::take_sigreturn_frame() {
        unsafe {
            let ctx = arch::syscall::get_user_context_mut();
            ctx.rip = frame.saved_rip;
            ctx.rsp = frame.saved_rsp;
            ctx.rflags = frame.saved_rflags;
            ctx.rax = frame.saved_rax;
            ctx.rbx = frame.saved_rbx;
            ctx.rcx = frame.saved_rcx;
            ctx.rdx = frame.saved_rdx;
            ctx.rsi = frame.saved_rsi;
            ctx.rdi = frame.saved_rdi;
            ctx.rbp = frame.saved_rbp;
            ctx.r8 = frame.saved_r8;
            ctx.r9 = frame.saved_r9;
            ctx.r10 = frame.saved_r10;
            ctx.r11 = frame.saved_r11;
            ctx.r12 = frame.saved_r12;
            ctx.r13 = frame.saved_r13;
            ctx.r14 = frame.saved_r14;
            ctx.r15 = frame.saved_r15;
        }
        // Restore signal mask
        let current_pid = sched::current_pid().unwrap_or(0);
        if current_pid > 1 {
            if let Some(meta_arc) = sched::get_task_meta(current_pid) {
                if let Some(mut meta) = meta_arc.try_lock() {
                    let mut restored_mask = frame.saved_mask;
                    restored_mask.remove(signal::SIGKILL);
                    restored_mask.remove(signal::SIGSTOP);
                    meta.signal_mask = restored_mask;
                }
            }
        }
        return; // Don't check for new signals — just return to pre-signal state
    }

    // — GraveShift: Reschedule check on syscall return.
    // SYSCALL_USER_CONTEXT is a *global* — another task's syscall_entry will
    // clobber it while we sleep. Save it to the kernel stack (per-task) and
    // restore after we're switched back in.
    if sched::need_resched() {
        // — GraveShift: Trace syscall-return yields — gated behind debug-syscall.
        // Fires on every preemption point, saturates serial when top runs.
        #[cfg(feature = "debug-syscall")]
        {
            let _dbg_pid = sched::current_pid().unwrap_or(99) as u8;
            unsafe {
                os_log::write_str_raw("[SCR] p=");
                if _dbg_pid >= 10 { os_log::write_byte_raw(b'0' + (_dbg_pid / 10)); }
                os_log::write_byte_raw(b'0' + (_dbg_pid % 10));
                os_log::write_str_raw("\n");
            }
        }
        let saved_ctx = unsafe { *arch::syscall::get_user_context_mut() };

        arch::allow_kernel_preempt();
        unsafe {
            core::arch::asm!("sti", "hlt", options(nomem, nostack));
        }
        arch::disallow_kernel_preempt();

        // — GraveShift: Resumed from yield — gated behind debug-syscall.
        #[cfg(feature = "debug-syscall")]
        unsafe {
            let _r_pid = sched::current_pid().unwrap_or(99) as u8;
            os_log::write_str_raw("[SCR] resume p=");
            if _r_pid >= 10 { os_log::write_byte_raw(b'0' + (_r_pid / 10)); }
            os_log::write_byte_raw(b'0' + (_r_pid % 10));
            os_log::write_str_raw("\n");
        }

        // — GraveShift: Re-disable interrupts. The asm caller (syscall_entry)
        // ran CLI before calling us; after sti+hlt+iretq interrupts are
        // enabled again. Restore the invariant before touching the global.
        unsafe {
            core::arch::asm!("cli", options(nomem, nostack));
        }

        unsafe {
            *arch::syscall::get_user_context_mut() = saved_ctx;
        }
    }

    // Get current PID (fast path, no locks)
    let current_pid = match sched::current_pid() {
        Some(pid) if pid > 1 => pid,
        _ => return, // Idle or init don't get signals
    };

    // Try to get process metadata
    let meta_arc = match sched::get_task_meta(current_pid) {
        Some(arc) => arc,
        None => return,
    };

    // Try to lock metadata (non-blocking to avoid deadlock)
    let mut meta = match meta_arc.try_lock() {
        Some(guard) => guard,
        None => {
            // — GraveShift: Trace try_lock failure — hunting signal delivery blackhole
            unsafe {
                os_log::write_str_raw("[SCR-SIG] try_lock FAIL p=");
                let pid_b = current_pid as u8;
                if pid_b >= 10 { os_log::write_byte_raw(b'0' + (pid_b / 10)); }
                os_log::write_byte_raw(b'0' + (pid_b % 10));
                os_log::write_str_raw("\n");
            }
            return;
        }
    };

    // Check if there are any deliverable signals
    if !meta.has_pending_signals() {
        return;
    }

    // — GraveShift: Signal detected! Trace everything so we can hunt down delivery failures.
    unsafe {
        os_log::write_str_raw("[SIGCHK] p=");
        let pid_b = current_pid as u8;
        if pid_b >= 10 { os_log::write_byte_raw(b'0' + (pid_b / 10)); }
        os_log::write_byte_raw(b'0' + (pid_b % 10));
        os_log::write_str_raw(" HAS pending\n");
    }

    let signal_mask = meta.signal_mask;

    // Dequeue the highest priority pending signal
    let pending = match meta.pending_signals.dequeue(&signal_mask) {
        Some(p) => p,
        None => {
            unsafe { os_log::write_str_raw("[SIGCHK] dequeue=NONE\n"); }
            return;
        }
    };

    let signo = pending.signo;

    unsafe {
        os_log::write_str_raw("[SIGCHK] sig=");
        os_log::write_byte_raw(b'0' + signo as u8);
        os_log::write_str_raw("\n");
    }

    // Get the signal action
    let action = if signo >= 1 && signo <= signal::NSIG as i32 {
        meta.sigactions[(signo - 1) as usize]
    } else {
        signal::SigAction::new()
    };

    // — GraveShift: Log the handler type so we know if SIG_IGN is eating our signals.
    unsafe {
        os_log::write_str_raw("[SIGCHK] handler=");
        let h = action.sa_handler;
        if h == 0 {
            os_log::write_str_raw("DFL");
        } else if h == 1 {
            os_log::write_str_raw("IGN");
        } else {
            os_log::write_str_raw("USR");
        }
        os_log::write_str_raw("\n");
    }

    // Determine what to do with this signal
    let result = determine_action(&pending, &action, &signal_mask);

    match result {
        SignalResult::Terminate | SignalResult::CoreDump => {
            // 🔥 GraveShift: Process gets the axe 🔥
            // — GraveShift: Trace signal kills so we catch who's getting axed
            unsafe {
                os_log::write_str_raw("[SIG] KILL p=");
                let pid_b = current_pid as u8;
                if pid_b >= 10 { os_log::write_byte_raw(b'0' + (pid_b / 10)); }
                os_log::write_byte_raw(b'0' + (pid_b % 10));
                os_log::write_str_raw(" sig=");
                os_log::write_byte_raw(b'0' + signo as u8);
                os_log::write_str_raw("\n");
            }
            // — GraveShift: Close all fds while we still hold the lock.
            // Same fix as user_exit() and kill_faulting_process() — zombie holding
            // PipeWrite = pipe deadlock. Kill them before we die.
            meta.fd_table = vfs::FdTable::new();
            meta.shared_fd_table = None;

            drop(meta); // Release lock before calling scheduler

            // — GraveShift: Linux waitpid format for signal kill:
            // bits [6:0] = signal number. WTERMSIG(status) = status & 0x7F.
            let exit_status = signo & 0x7F;
            sched::set_task_exit_status(current_pid, exit_status);

            // Wake parent to reap us
            if let Some(ppid) = sched::get_task_ppid(current_pid) {
                if ppid > 0 {
                    wake_up(ppid);
                }
            }

            // Block ourselves (we're now a zombie)
            sched::block_current(TaskState::TASK_ZOMBIE);

            // — GraveShift: CRITICAL — must yield CPU here. If we return, the asm does
            // sysretq and the zombie runs free in user mode. nanosleep then overwrites
            // TASK_ZOMBIE with TASK_INTERRUPTIBLE, and the sleep queue keeps waking us.
            // The process lives forever like an unkillable revenant. Yield now so the
            // scheduler context-switches us out permanently. The zombie never wakes.
            unsafe {
                os_log::write_str_raw("[SIG] yielding zombie p=");
                let pid_b = current_pid as u8;
                if pid_b >= 10 { os_log::write_byte_raw(b'0' + (pid_b / 10)); }
                os_log::write_byte_raw(b'0' + (pid_b % 10));
                os_log::write_str_raw("\n");
            }
            sched::set_need_resched();
            arch::allow_kernel_preempt();
            unsafe {
                core::arch::asm!("sti", "hlt", options(nomem, nostack));
            }
            // — GraveShift: If we somehow resume (shouldn't happen — nobody reschedules
            // a zombie), loop forever so we never escape to user mode.
            loop {
                unsafe { core::arch::asm!("cli", "hlt", options(nomem, nostack)); }
            }
        }
        SignalResult::UserHandler {
            handler,
            signo,
            info,
            flags: _,
            handler_mask,
        } => {
            // 🔥 GraveShift: Redirect to user's signal handler 🔥

            unsafe {
                let ctx = arch::syscall::get_user_context_mut();

                // Build saved registers from current context
                let regs = signal::delivery::SavedRegisters {
                    rax: ctx.rax,
                    rbx: ctx.rbx,
                    rcx: ctx.rcx,
                    rdx: ctx.rdx,
                    rsi: ctx.rsi,
                    rdi: ctx.rdi,
                    rbp: ctx.rbp,
                    r8: ctx.r8,
                    r9: ctx.r9,
                    r10: ctx.r10,
                    r11: ctx.r11,
                    r12: ctx.r12,
                    r13: ctx.r13,
                    r14: ctx.r14,
                    r15: ctx.r15,
                };

                // Get signal restorer from action
                let restorer = action.sa_restorer;

                // Setup signal frame
                let (new_rip, new_rsp, sig_frame) = signal::delivery::setup_signal_handler(
                    handler,
                    signo,
                    info,
                    action.flags(),
                    restorer,
                    meta.signal_mask,
                    ctx.rip,
                    ctx.rsp,
                    ctx.rflags,
                    &regs,
                );

                // Write signal frame to user stack
                // SMAP user access (STAC) is already enabled in syscall_entry
                let frame_ptr = new_rsp as *mut signal::delivery::SignalFrame;
                core::ptr::write(frame_ptr, sig_frame);

                // Update process signal mask for handler execution
                meta.signal_mask = handler_mask;

                // Modify saved context to redirect to signal handler on sysret
                ctx.rip = new_rip;
                ctx.rsp = new_rsp;
                ctx.rdi = signo as u64; // First argument: signal number
            }
        }
        SignalResult::Ignore => {
            // — GraveShift: Trace ignored signals so we can catch SIG_IGN stealing our kills.
            unsafe {
                os_log::write_str_raw("[SIGCHK] IGNORED sig=");
                os_log::write_byte_raw(b'0' + signo as u8);
                os_log::write_str_raw(" p=");
                let pid_b = current_pid as u8;
                if pid_b >= 10 { os_log::write_byte_raw(b'0' + (pid_b / 10)); }
                os_log::write_byte_raw(b'0' + (pid_b % 10));
                os_log::write_str_raw("\n");
            }
        }
        SignalResult::Stop => {
            // — ThreadRogue: Freeze the process
            meta.stop_signal = Some(signo as u8);
            meta.continued = false;

            let ppid = sched::get_task_ppid(current_pid);
            drop(meta);

            // Block the task
            sched::block_current(TaskState::TASK_STOPPED);

            // Wake parent for WUNTRACED waitpid
            if let Some(ppid) = ppid {
                if ppid > 0 {
                    wake_up(ppid);
                }
            }

            // — ThreadRogue: Same as Terminate — must yield or the stopped process
            // returns to user mode and nanosleep overwrites TASK_STOPPED.
            sched::set_need_resched();
            arch::allow_kernel_preempt();
            unsafe {
                core::arch::asm!("sti", "hlt", options(nomem, nostack));
            }
            arch::disallow_kernel_preempt();
            // — ThreadRogue: For stopped tasks, we CAN resume here after SIGCONT.
        }
        SignalResult::Continue => {
            // — ThreadRogue: Thaw the process
            meta.stop_signal = None;
            meta.continued = true;

            let ppid = sched::get_task_ppid(current_pid);
            drop(meta);

            // If we were stopped, wake up
            if sched::get_task_state(current_pid) == Some(TaskState::TASK_STOPPED) {
                wake_up(current_pid);
            }

            // Wake parent for WCONTINUED waitpid
            if let Some(ppid) = ppid {
                if ppid > 0 {
                    wake_up(ppid);
                }
            }
        }
        SignalResult::None => {
            // No signal - shouldn't happen
        }
    }
}
