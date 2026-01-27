//! Preemptive scheduler for the OXIDE kernel.
//!
//! This module provides the bridge between the new Linux-model scheduler
//! (in the sched crate) and the kernel's process management.
//!
//! The sched crate handles scheduling decisions (which task to run),
//! while this module handles the actual context switching using the
//! process table from the proc crate.

extern crate alloc;

use alloc::string::ToString;
use alloc::sync::Arc;
use alloc::vec::Vec;
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

/// Debug: Check physical memory at 0xfec5000
fn debug_check_0xfec5(label: &str) {
    let check_addr = 0xfec5000u64;
    let check_virt = phys_to_virt(PhysAddr::new(check_addr));
    let check_next = unsafe { *(check_virt.as_ptr::<u64>()) };
    arch::serial::write_str("[SCHED] ");
    arch::serial::write_str(label);
    arch::serial::write_str(": 0xfec5.next=0x");
    // Print hex manually
    for i in (0..16).rev() {
        let nibble = ((check_next >> (i * 4)) & 0xF) as u8;
        let c = if nibble < 10 { b'0' + nibble } else { b'a' + (nibble - 10) };
        arch::serial::write_byte(c);
    }
    arch::serial::write_str("\n");
}

/// Initialize the scheduler for the current CPU
///
/// Should be called early during kernel initialization.
pub fn init() {
    debug_check_0xfec5("pre-init_cpu");

    // Initialize the scheduler for CPU 0 with idle task PID 0
    sched::init_cpu(0, 0);

    debug_check_0xfec5("post-init_cpu");

    // Create a real idle task with PID 0
    // The idle task doesn't need a real kernel stack since it runs on the BSP stack
    // We use a placeholder address that won't be used for context switches to idle

    // Debug: Check both 0xfec4 and 0xfec5 before and after each step
    fn debug_check_both(label: &str) {
        let check5_virt = phys_to_virt(PhysAddr::new(0xfec5000));
        let check5_next = unsafe { *(check5_virt.as_ptr::<u64>()) };
        let check4_virt = phys_to_virt(PhysAddr::new(0xfec4000));
        let check4_next = unsafe { *(check4_virt.as_ptr::<u64>()) };
        arch::serial::write_str("[SCHED2] ");
        arch::serial::write_str(label);
        arch::serial::write_str(": 0xfec5=0x");
        for i in (0..16).rev() {
            let nibble = ((check5_next >> (i * 4)) & 0xF) as u8;
            arch::serial::write_byte(if nibble < 10 { b'0' + nibble } else { b'a' + (nibble - 10) });
        }
        arch::serial::write_str(" 0xfec4=0x");
        for i in (0..16).rev() {
            let nibble = ((check4_next >> (i * 4)) & 0xF) as u8;
            arch::serial::write_byte(if nibble < 10 { b'0' + nibble } else { b'a' + (nibble - 10) });
        }
        arch::serial::write_str("\n");
    }

    debug_check_both("pre-new_kernel");
    let meta = ProcessMeta::new_kernel();
    debug_check_both("post-new_kernel");

    debug_check_both("pre-Mutex::new");
    let mutex = Mutex::new(meta);
    debug_check_both("post-Mutex::new");

    let idle_meta = Arc::new(mutex);
    debug_check_both("post-Arc::new");

    debug_check_0xfec5("post-ProcessMeta::new_kernel");

    // Create idle task with ProcessMeta
    let idle_task = Task::new_idle_with_meta(
        0,                         // PID 0 is the idle task
        0,                         // CPU 0
        PhysAddr::new(0),          // No separate kernel stack needed
        0,                         // Stack size (idle uses BSP stack)
        idle_meta,
    );

    debug_check_0xfec5("post-new_idle_with_meta");

    // Add the idle task to the scheduler
    sched::add_task(idle_task);

    debug_check_0xfec5("post-add_task");
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

    // Update kernel stack pointers (only needed if switching to user mode)
    if next_ctx.cs == 0x23 {
        unsafe {
            arch::syscall::set_kernel_stack(kernel_stack_top);
        }
        arch::gdt::set_kernel_stack(kernel_stack_top);
    }

    // Switch page tables
    unsafe {
        core::arch::asm!("mov cr3, {}", in(reg) next_pml4.as_u64());
    }

    // Build interrupt frame for next process
    let new_frame_ptr =
        (kernel_stack_top - core::mem::size_of::<InterruptFrame>() as u64) as *mut InterruptFrame;

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
/// Switches to another ready task if one exists.
/// Returns 0 on success.
pub fn kernel_yield() -> i64 {
    let current_pid = sched::current_pid().unwrap_or(0);

    // Tell sched crate we're yielding
    sched::yield_current();

    // Find next task to run
    let next_pid = pick_next_process(current_pid);

    if next_pid == current_pid {
        return 0; // No other tasks
    }

    // Get current process's user context from syscall entry
    let user_ctx = arch::get_user_context();

    // Save current task context to scheduler
    let current_ctx = sched::TaskContext {
        rip: user_ctx.rip,
        rsp: user_ctx.rsp,
        rflags: user_ctx.rflags,
        rax: 0, // sched_yield returns 0
        rbx: user_ctx.rbx,
        rcx: user_ctx.rcx,
        rdx: user_ctx.rdx,
        rsi: user_ctx.rsi,
        rdi: user_ctx.rdi,
        rbp: user_ctx.rbp,
        r8: user_ctx.r8,
        r9: user_ctx.r9,
        r10: user_ctx.r10,
        r11: user_ctx.r11,
        r12: user_ctx.r12,
        r13: user_ctx.r13,
        r14: user_ctx.r14,
        r15: user_ctx.r15,
        cs: 0x23, // User mode
        ss: 0x1B,
    };
    sched::set_task_context(current_pid, current_ctx);

    // Get next task's context switch info from the scheduler
    let (next_ctx, next_pml4, kernel_stack, kernel_stack_size) =
        match sched::get_task_switch_info(next_pid) {
            Some(info) => info,
            None => return 0,
        };

    let kernel_stack_top = {
        let ks_virt = phys_to_virt(kernel_stack);
        ks_virt.as_u64() + kernel_stack_size as u64
    };

    // Switch to next process via scheduler
    sched::switch_to(next_pid);

    // Update kernel stack pointers
    unsafe {
        arch::syscall::set_kernel_stack(kernel_stack_top);
    }
    arch::gdt::set_kernel_stack(kernel_stack_top);

    // Switch page tables and return to next process via sysretq
    // Use TaskContext directly - same layout as ProcessContext
    static mut YIELD_CTX: sched::TaskContext = sched::TaskContext {
        rip: 0,
        rsp: 0,
        rflags: 0,
        rax: 0,
        rbx: 0,
        rcx: 0,
        rdx: 0,
        rsi: 0,
        rdi: 0,
        rbp: 0,
        r8: 0,
        r9: 0,
        r10: 0,
        r11: 0,
        r12: 0,
        r13: 0,
        r14: 0,
        r15: 0,
        cs: 0,
        ss: 0,
    };

    unsafe {
        use core::ptr::addr_of_mut;
        *addr_of_mut!(YIELD_CTX) = next_ctx;

        // Switch page tables
        core::arch::asm!("mov cr3, {}", in(reg) next_pml4.as_u64());

        let ctx_ptr = addr_of_mut!(YIELD_CTX) as u64;

        core::arch::asm!(
            "mov rax, {ctx}",
            "mov rcx, [rax]",       // rip -> rcx for sysret
            "mov r11, [rax + 16]",  // rflags -> r11 for sysret
            "or r11, 0x200",        // Ensure IF set
            "mov rbx, [rax + 32]",
            "mov rbp, [rax + 72]",
            "mov r12, [rax + 112]",
            "mov r13, [rax + 120]",
            "mov r14, [rax + 128]",
            "mov r15, [rax + 136]",
            "mov rdi, [rax + 64]",
            "mov rsi, [rax + 56]",
            "mov rdx, [rax + 48]",
            "mov r8, [rax + 80]",
            "mov r9, [rax + 88]",
            "mov r10, [rax + 96]",
            "push qword ptr [rax + 8]",
            "mov rax, [rax + 24]",
            "pop rsp",
            "swapgs",
            "sysretq",
            ctx = in(reg) ctx_ptr,
            options(noreturn)
        );
    }
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
