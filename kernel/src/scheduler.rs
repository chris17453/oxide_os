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
extern "C" fn idle_loop() -> ! {
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

    // Set the idle task's RIP to the idle_loop function
    let mut ctx = idle_task.context;
    ctx.rip = idle_loop as *const () as u64;
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
    sched::remove_task(pid);
}

/// Scheduler tick callback - called from timer interrupt at 100Hz
///
/// Implements preemptive scheduling using the sched crate.
/// Returns the RSP to restore (may be different if we switched processes).
pub fn scheduler_tick(current_rsp: u64) -> u64 {
    // Check sleep queue and wake any tasks whose sleep time has expired
    syscall::time::check_sleepers();

    // Debug: full scheduler dump every ~3 seconds (300 ticks)
    #[cfg(feature = "debug-sched")]
    {
        let ticks = arch::timer_ticks();
        if ticks % 300 == 0 {
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
            if let Some(mut meta) = meta_arc.try_lock() {
                if meta.has_pending_signals() {
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
                                // Release ProcessMeta lock before calling scheduler functions
                                drop(meta);

                                // Exit status: 128 + signal number (Unix convention)
                                let exit_status = 128 + signo;
                                sched::set_task_exit_status(current_pid, exit_status);

                                // Wake parent so it can reap via wait()
                                if let Some(ppid) = sched::get_task_ppid(current_pid) {
                                    if ppid > 0 {
                                        wake_up(ppid);
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
                                // — ThreadRogue: freezing the process in its tracks
                                meta.stop_signal = Some(signo as u8);
                                meta.continued = false;

                                // Wake parent for WUNTRACED waitpid
                                let ppid_opt = sched::get_task_ppid(current_pid);
                                drop(meta);

                                sched::block_current(TaskState::TASK_STOPPED);

                                if let Some(ppid) = ppid_opt {
                                    if ppid > 0 {
                                        wake_up(ppid);
                                    }
                                }
                            }
                            SignalResult::Continue => {
                                // — ThreadRogue: thawing from the ice
                                meta.stop_signal = None;
                                meta.continued = true;

                                let ppid_opt = sched::get_task_ppid(current_pid);
                                drop(meta);

                                // If task was stopped, wake it back up
                                if sched::get_task_state(current_pid)
                                    == Some(TaskState::TASK_STOPPED)
                                {
                                    wake_up(current_pid);
                                }

                                // Wake parent for WCONTINUED waitpid
                                if let Some(ppid) = ppid_opt {
                                    if ppid > 0 {
                                        wake_up(ppid);
                                    }
                                }
                            }
                            SignalResult::None => {
                                // No signal - shouldn't happen here
                            }
                        }
                    }
                }
            }
        }
    }

    // Check if kernel code has explicitly allowed preemption (e.g., poll, nanosleep)
    let kernel_preempt_ok = arch::is_kernel_preempt_allowed();

    // Only preempt:
    // - User mode (CS = 0x23) - always safe
    // - Kernel mode if KERNEL_PREEMPT_OK flag is set (blocking syscalls)
    let in_kernel = frame.cs != 0x23;
    if in_kernel && !kernel_preempt_ok {
        // Still tick the scheduler to update accounting
        sched::scheduler_tick();
        return current_rsp;
    }

    // Tick the scheduler - this updates vruntime, checks for preemption, etc.
    // Note: Do NOT clear kernel_preempt flag here. If try_with_rq fails
    // (lock contention from yield_current loop), clearing the flag would
    // permanently prevent preemption for this task. The flag is only cleared
    // when we actually perform a context switch (below).
    let need_resched = sched::scheduler_tick();

    if !need_resched && !sched::need_resched() {
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

    // We're actually switching — clear the kernel preempt flag now.
    // The task we switch TO will set it itself if it needs preemption
    // (e.g., via allow_kernel_preempt in blocking syscalls or idle loop).
    if kernel_preempt_ok {
        arch::clear_kernel_preempt();
    }

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

    // Save current task context from interrupt frame to scheduler's Task
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
    };
    sched::set_task_context(current_pid, current_ctx);

    // Get next task's context switch info from the scheduler
    let (next_ctx, next_pml4, kernel_stack, kernel_stack_size) =
        match sched::get_task_switch_info(next_pid) {
            Some(info) => info,
            None => return current_rsp,
        };

    let kernel_stack_top = {
        let ks_virt = phys_to_virt(kernel_stack);
        ks_virt.as_u64() + kernel_stack_size as u64
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

    // Switch to next process via scheduler
    // The scheduler handles state updates internally
    sched::switch_to(next_pid);

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

    // Switch page tables
    unsafe {
        core::arch::asm!("mov cr3, {}", in(reg) next_pml4.as_u64());
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
    let is_kernel_mode = next_ctx.cs == 0x08 || (next_ctx.cs == 0 && frame.cs == 0x08);
    let frame_size = core::mem::size_of::<InterruptFrame>() as u64;
    let new_frame_ptr = if is_kernel_mode {
        // Place below the task's saved kernel RSP (where original interrupt frame was)
        (next_ctx.rsp - frame_size) as *mut InterruptFrame
    } else {
        // User-mode: top of kernel stack is safe
        (kernel_stack_top - frame_size) as *mut InterruptFrame
    };

    unsafe {
        let ss = if next_ctx.ss != 0 { next_ctx.ss } else { 0x1B };
        let cs = if next_ctx.cs != 0 { next_ctx.cs } else { 0x23 };

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
    // Use the actual scheduler we built!
    if let Some(next_pid) = sched::pick_next_task() {
        // Validate that this PID exists (has ProcessMeta) in the unified model
        if sched::get_task_meta(next_pid).is_some() {
            return next_pid;
        }
    }

    // Fallback to current if scheduler returns nothing valid
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
        current_pid, signo, rip
    );

    let exit_status = 128 + signo as i32;
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
        unsafe { core::arch::asm!("sti", "hlt", options(nomem, nostack)); }
    }
}

/// Checks for deliverable signals and modifies the syscall return context
/// to redirect to signal handlers if needed.
///
/// -- GraveShift: Signal delivery on syscall return, not just timer ticks
pub fn check_signals_on_syscall_return() {
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
        None => return, // Can't lock, skip for now (will be delivered on next opportunity)
    };

    // Check if there are any deliverable signals
    if !meta.has_pending_signals() {
        return;
    }

    let signal_mask = meta.signal_mask;
    
    // Dequeue the highest priority pending signal
    let pending = match meta.pending_signals.dequeue(&signal_mask) {
        Some(p) => p,
        None => return,
    };

    let signo = pending.signo;
    
    // Get the signal action
    let action = if signo >= 1 && signo <= signal::NSIG as i32 {
        meta.sigactions[(signo - 1) as usize]
    } else {
        signal::SigAction::new()
    };

    // Determine what to do with this signal
    let result = determine_action(&pending, &action, &signal_mask);

    match result {
        SignalResult::Terminate | SignalResult::CoreDump => {
            // 🔥 GraveShift: Process gets the axe 🔥
            drop(meta); // Release lock before calling scheduler

            // Exit with signal status (128 + signal number)
            let exit_status = 128 + signo;
            sched::set_task_exit_status(current_pid, exit_status);

            // Wake parent to reap us
            if let Some(ppid) = sched::get_task_ppid(current_pid) {
                if ppid > 0 {
                    wake_up(ppid);
                }
            }

            // Block ourselves (we're now a zombie)
            sched::block_current(TaskState::TASK_ZOMBIE);
            
            // Note: When we return from syscall, scheduler will switch us out
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
            // Signal ignored - nothing to do
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
