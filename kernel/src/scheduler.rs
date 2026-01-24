//! Preemptive scheduler for the OXIDE kernel.
//!
//! Implements round-robin scheduling via timer interrupts.

use arch_x86_64 as arch;
use mm_paging::phys_to_virt;
use proc::process_table;

use crate::globals::READY_QUEUE;

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

/// Scheduler tick callback - called from timer interrupt at 100Hz
///
/// Implements round-robin preemptive scheduling.
/// Returns the RSP to restore (may be different if we switched processes).
pub fn scheduler_tick(current_rsp: u64) -> u64 {
    let frame = unsafe { &*(current_rsp as *const InterruptFrame) };

    // Only preempt user mode (CS = 0x23)
    // Don't preempt kernel - could cause issues with locks
    if frame.cs != 0x23 {
        return current_rsp;
    }

    let table = process_table();
    let current_pid = table.current_pid();

    // Check if there's another process to run
    let next_pid = {
        let mut queue = READY_QUEUE.lock();
        if queue.is_empty() {
            return current_rsp; // No other processes
        }
        queue.remove(0) // Take first (round-robin)
    };

    // Don't switch to self
    if next_pid == current_pid {
        READY_QUEUE.lock().push(next_pid);
        return current_rsp;
    }

    // Save current process context from interrupt frame
    if let Some(current) = table.get(current_pid) {
        let mut proc = current.lock();
        let ctx = proc.context_mut();
        ctx.rip = frame.rip;
        ctx.rsp = frame.rsp;
        ctx.rflags = frame.rflags;
        ctx.rax = frame.rax;
        ctx.rbx = frame.rbx;
        ctx.rcx = frame.rcx;
        ctx.rdx = frame.rdx;
        ctx.rsi = frame.rsi;
        ctx.rdi = frame.rdi;
        ctx.rbp = frame.rbp;
        ctx.r8 = frame.r8;
        ctx.r9 = frame.r9;
        ctx.r10 = frame.r10;
        ctx.r11 = frame.r11;
        ctx.r12 = frame.r12;
        ctx.r13 = frame.r13;
        ctx.r14 = frame.r14;
        ctx.r15 = frame.r15;

        // Put current process back in ready queue
        READY_QUEUE.lock().push(current_pid);
    }

    // Get next process info
    let (next_ctx, next_pml4, kernel_stack_top) = {
        let next = match table.get(next_pid) {
            Some(p) => p,
            None => return current_rsp,
        };
        let proc = next.lock();
        (
            proc.context().clone(),
            proc.address_space().pml4_phys(),
            {
                let ks_virt = phys_to_virt(proc.kernel_stack());
                ks_virt.as_u64() + proc.kernel_stack_size() as u64
            },
        )
    };

    // Switch to next process
    table.set_current_pid(next_pid);

    // Update kernel stack pointers
    unsafe {
        arch::syscall::set_kernel_stack(kernel_stack_top);
    }
    arch::gdt::set_kernel_stack(kernel_stack_top);

    // Switch page tables
    unsafe {
        core::arch::asm!("mov cr3, {}", in(reg) next_pml4.as_u64());
    }

    // Build interrupt frame for next process
    let new_frame_ptr = (kernel_stack_top - core::mem::size_of::<InterruptFrame>() as u64)
        as *mut InterruptFrame;

    unsafe {
        (*new_frame_ptr).ss = 0x1B; // USER_DS
        (*new_frame_ptr).rsp = next_ctx.rsp;
        (*new_frame_ptr).rflags = next_ctx.rflags | 0x200; // Ensure IF set
        (*new_frame_ptr).cs = 0x23; // USER_CS
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

/// Voluntary yield - called from sched_yield syscall
///
/// Switches to another ready process if one exists.
/// Returns 0 on success.
pub fn kernel_yield() -> i64 {
    let table = process_table();
    let current_pid = table.current_pid();

    // Check if there's another process to run
    let next_pid = {
        let mut queue = READY_QUEUE.lock();
        if queue.is_empty() {
            return 0; // No other processes, nothing to yield to
        }
        queue.remove(0) // Take first (round-robin)
    };

    // Don't switch to self
    if next_pid == current_pid {
        READY_QUEUE.lock().push(next_pid);
        return 0;
    }

    // Get current process's user context from syscall entry
    let user_ctx = arch::get_user_context();

    // Save current process context
    if let Some(current) = table.get(current_pid) {
        let mut proc = current.lock();
        let ctx = proc.context_mut();
        ctx.rip = user_ctx.rip;
        ctx.rsp = user_ctx.rsp;
        ctx.rflags = user_ctx.rflags;
        ctx.rax = 0; // sched_yield returns 0
        ctx.rbx = user_ctx.rbx;
        ctx.rcx = user_ctx.rcx;
        ctx.rdx = user_ctx.rdx;
        ctx.rsi = user_ctx.rsi;
        ctx.rdi = user_ctx.rdi;
        ctx.rbp = user_ctx.rbp;
        ctx.r8 = user_ctx.r8;
        ctx.r9 = user_ctx.r9;
        ctx.r10 = user_ctx.r10;
        ctx.r11 = user_ctx.r11;
        ctx.r12 = user_ctx.r12;
        ctx.r13 = user_ctx.r13;
        ctx.r14 = user_ctx.r14;
        ctx.r15 = user_ctx.r15;

        // Put current process back in ready queue
        READY_QUEUE.lock().push(current_pid);
    }

    // Get next process info
    let (next_ctx, next_pml4, kernel_stack_top) = {
        let next = match table.get(next_pid) {
            Some(p) => p,
            None => return 0,
        };
        let proc = next.lock();
        (
            proc.context().clone(),
            proc.address_space().pml4_phys(),
            {
                let ks_virt = phys_to_virt(proc.kernel_stack());
                ks_virt.as_u64() + proc.kernel_stack_size() as u64
            },
        )
    };

    // Switch to next process
    table.set_current_pid(next_pid);

    // Update kernel stack pointers
    unsafe {
        arch::syscall::set_kernel_stack(kernel_stack_top);
    }
    arch::gdt::set_kernel_stack(kernel_stack_top);

    // Switch page tables and return to next process via sysretq
    // Use a static to hold context since we run out of registers
    static mut YIELD_CTX: proc::ProcessContext = proc::ProcessContext {
        rip: 0, rsp: 0, rflags: 0, rax: 0, rbx: 0, rcx: 0, rdx: 0,
        rsi: 0, rdi: 0, rbp: 0, r8: 0, r9: 0, r10: 0, r11: 0,
        r12: 0, r13: 0, r14: 0, r15: 0,
    };

    unsafe {
        use core::ptr::addr_of_mut;
        *addr_of_mut!(YIELD_CTX) = next_ctx;

        // Switch page tables
        core::arch::asm!("mov cr3, {}", in(reg) next_pml4.as_u64());

        // Get context pointer
        let ctx_ptr = addr_of_mut!(YIELD_CTX) as u64;

        // Restore registers from context and sysretq
        // Context layout: rip(0), rsp(8), rflags(16), rax(24), rbx(32), rcx(40), rdx(48),
        //                 rsi(56), rdi(64), rbp(72), r8(80), r9(88), r10(96), r11(104),
        //                 r12(112), r13(120), r14(128), r15(136)
        core::arch::asm!(
            // Use rax as context pointer
            "mov rax, {ctx}",
            // Load sysret values first
            "mov rcx, [rax]",       // rip -> rcx for sysret
            "mov r11, [rax + 16]",  // rflags -> r11 for sysret
            "or r11, 0x200",        // Ensure IF set
            // Load callee-saved registers
            "mov rbx, [rax + 32]",
            "mov rbp, [rax + 72]",
            "mov r12, [rax + 112]",
            "mov r13, [rax + 120]",
            "mov r14, [rax + 128]",
            "mov r15, [rax + 136]",
            // Load caller-saved
            "mov rdi, [rax + 64]",
            "mov rsi, [rax + 56]",
            "mov rdx, [rax + 48]",
            "mov r8, [rax + 80]",
            "mov r9, [rax + 88]",
            "mov r10, [rax + 96]",
            // Load user rsp (do this before loading rax!)
            "push qword ptr [rax + 8]",  // Push user rsp
            // Load return value
            "mov rax, [rax + 24]",
            // Load user rsp and sysret
            "pop rsp",
            "swapgs",
            "sysretq",
            ctx = in(reg) ctx_ptr,
            options(noreturn)
        );
    }
}
