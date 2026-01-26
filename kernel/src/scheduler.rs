//! Preemptive scheduler for the OXIDE kernel.
//!
//! This module provides the bridge between the new Linux-model scheduler
//! (in the sched crate) and the kernel's process management.
//!
//! The sched crate handles scheduling decisions (which task to run),
//! while this module handles the actual context switching using the
//! process table from the proc crate.

use arch_x86_64 as arch;
use mm_paging::phys_to_virt;
use proc::process_table;
use proc_traits::ProcessState;
use sched::{self, SchedPolicy, TaskState};

fn update_process_state(pid: u32, state: ProcessState) {
    if pid == 0 {
        return;
    }
    let table = process_table();
    if let Some(proc_arc) = table.get(pid) {
        let mut proc = proc_arc.lock();
        // Don't resurrect zombies or exited tasks
        if proc.state() == ProcessState::Zombie && state == ProcessState::Ready {
            return;
        }
        proc.set_state(state);
    }
}

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
}

/// Add a process to the scheduler
///
/// Called when a new process is created (fork/exec).
pub fn add_process(pid: u32) {
    let table = process_table();

    if let Some(proc_arc) = table.get(pid) {
        let mut proc = proc_arc.lock();

        // Newly created processes are ready to be scheduled
        proc.set_state(ProcessState::Ready);

        // Create a scheduler task for this process
        let mut task = sched::create_task(
            pid,
            proc.ppid(),
            proc.kernel_stack(),
            proc.kernel_stack_size(),
            proc.address_space().pml4_phys(),
            proc.entry_point().as_u64(),
            proc.user_stack_top().as_u64(),
        );

        // Map process nice value to scheduler nice
        task.set_nice(proc.nice() as i8);

        // Copy context from process to task
        let ctx = proc.context();
        task.context_mut().rip = ctx.rip;
        task.context_mut().rsp = ctx.rsp;
        task.context_mut().rflags = ctx.rflags;
        task.context_mut().rax = ctx.rax;
        task.context_mut().rbx = ctx.rbx;
        task.context_mut().rcx = ctx.rcx;
        task.context_mut().rdx = ctx.rdx;
        task.context_mut().rsi = ctx.rsi;
        task.context_mut().rdi = ctx.rdi;
        task.context_mut().rbp = ctx.rbp;
        task.context_mut().r8 = ctx.r8;
        task.context_mut().r9 = ctx.r9;
        task.context_mut().r10 = ctx.r10;
        task.context_mut().r11 = ctx.r11;
        task.context_mut().r12 = ctx.r12;
        task.context_mut().r13 = ctx.r13;
        task.context_mut().r14 = ctx.r14;
        task.context_mut().r15 = ctx.r15;
        task.context_mut().cs = ctx.cs;
        task.context_mut().ss = ctx.ss;

        // All new processes start as Normal/CFS
        task.policy = SchedPolicy::Normal;

        // Add to scheduler
        drop(proc);
        sched::add_task(task);
    }
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

    let table = process_table();
    let current_pid = table.current_pid();

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

    // Switch to next process
    if current_pid != next_pid {
        update_process_state(current_pid, ProcessState::Ready);
    }

    table.set_current_pid(next_pid);
    update_process_state(next_pid, ProcessState::Running);

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
        // Validate that this PID exists in the process table
        let table = process_table();
        if table.get(next_pid).is_some() {
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
    let table = process_table();

    if let Some(proc_arc) = table.get(pid) {
        let mut proc = proc_arc.lock();
        if proc.state() == ProcessState::Blocked {
            proc.clear_waiting();

            // Also notify the sched crate
            drop(proc);
            sched::wake_up(pid);
        }
    }
}

/// Wake up a process that was blocked waiting for a child
///
/// Called from user_exit when a child process exits.
pub fn wake_parent(parent_pid: u32) {
    wake_up(parent_pid);
}

/// Block the current process
///
/// Sets the process to blocked state in both ProcessTable and sched crate.
pub fn block_current(state: TaskState) {
    let table = process_table();
    let current_pid = table.current_pid();

    if let Some(proc_arc) = table.get(current_pid) {
        let mut proc = proc_arc.lock();
        proc.set_state(ProcessState::Blocked);
    }

    sched::block_current(state);
}

/// Voluntary yield - called from sched_yield syscall
///
/// Switches to another ready task if one exists.
/// Returns 0 on success.
pub fn kernel_yield() -> i64 {
    let table = process_table();
    let current_pid = table.current_pid();

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

    // Switch to next process
    update_process_state(current_pid, ProcessState::Ready);
    table.set_current_pid(next_pid);
    update_process_state(next_pid, ProcessState::Running);

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
