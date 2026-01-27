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

/// Initialize the scheduler for the current CPU
///
/// Should be called early during kernel initialization.
pub fn init() {
    // Initialize the scheduler for CPU 0 with idle task PID 0
    sched::init_cpu(0, 0);

    // Create a real idle task with PID 0
    // The idle task doesn't need a real kernel stack since it runs on the BSP stack
    // We use a placeholder address that won't be used for context switches to idle
    let meta = ProcessMeta::new_kernel();
    let idle_meta = Arc::new(Mutex::new(meta));

    // Create idle task with ProcessMeta
    let idle_task = Task::new_idle_with_meta(
        0,                         // PID 0 is the idle task
        0,                         // CPU 0
        PhysAddr::new(0),          // No separate kernel stack needed
        0,                         // Stack size (idle uses BSP stack)
        idle_meta,
    );

    // Add the idle task to the scheduler
    sched::add_task(idle_task);
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

    let frame = unsafe { &*(current_rsp as *const InterruptFrame) };

    // Use lock-free current PID to avoid deadlock in interrupt context
    let current_pid = sched::current_pid_lockfree().unwrap_or(0);

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

    // Clear the preempt flag after checking (it's per-yield-point)
    if kernel_preempt_ok {
        arch::clear_kernel_preempt();
    }

    // Tick the scheduler - this updates vruntime, checks for preemption, etc.
    let need_resched = sched::scheduler_tick();

    if !need_resched && !sched::need_resched() {
        return current_rsp;
    }

    // Find next task to run using the scheduler
    let next_pid = pick_next_process(current_pid);

    if next_pid == current_pid {
        sched::set_need_resched();
        return current_rsp;
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
    // Clear waiting state and wake via scheduler
    sched::clear_task_waiting(pid);
    sched::wake_up(pid);
}

/// Wake up a process that was blocked waiting for a child
///
/// Called from user_exit when a child process exits.
pub fn wake_parent(parent_pid: u32) {
    wake_up(parent_pid);
}

/// Block the current process
///
/// Sets the task to blocked state via scheduler.
pub fn block_current(state: TaskState) {
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

    // Brief halt to give the timer a chance to fire and switch us
    unsafe {
        core::arch::asm!("sti", options(nomem, nostack, preserves_flags));
        core::arch::asm!("hlt", options(nomem, nostack));
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
