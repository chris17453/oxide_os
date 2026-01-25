//! Process management callbacks for the OXIDE kernel.
//!
//! Implements fork, exec, wait, and exit syscall handlers.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;
use core::ptr::addr_of_mut;
use core::sync::atomic::Ordering;

use arch_traits::Arch;
use arch_x86_64 as arch;
use arch_x86_64::serial;
use mm_paging::{flush_tlb_all, phys_to_virt, write_cr3};
use os_core::PhysAddr;
use proc::{ProcessContext, WaitOptions, do_exec, do_fork, do_waitpid, process_table};
use proc_traits::Pid;
use vfs::{VnodeType, mount::GLOBAL_VFS};

#[allow(unused_imports)]
use crate::debug_fork;
use crate::globals::{
    CHILD_DONE, KERNEL_PML4, PARENT_CONTEXT, ParentContext, USER_EXIT_STATUS,
    USER_EXITED,
};
use crate::memory::FrameAllocatorWrapper;
use crate::scheduler::{wake_parent, add_process};
use sched::TaskContext;

/// User exit function
pub fn user_exit(status: i32) -> ! {
    // Get current process and mark as zombie
    let table = process_table();
    let current_pid = table.current_pid();

    // Debug: Print exit info
    {
        let mut writer = serial::SerialWriter;
        let _ = writeln!(
            writer,
            "[EXIT] Process {} exiting with status {}",
            current_pid, status
        );
    }

    // Debug: Print framebuffer write stats on process exit
    {
        let (writes, bytes, base) = devfs::devices::get_fb_write_stats();
        let mut writer = serial::SerialWriter;
        let _ = writeln!(
            writer,
            "[FB_DEBUG] Process {} exiting - FB writes={} bytes={} base={:#x}",
            current_pid, writes, bytes, base
        );
    }

    // Get parent PID before marking as zombie
    let parent_pid = if let Some(proc) = table.get(current_pid) {
        let mut p = proc.lock();
        let ppid = p.ppid();
        p.exit(status);
        ppid
    } else {
        0
    };

    unsafe {
        USER_EXIT_STATUS = status;
    }
    USER_EXITED.store(true, Ordering::SeqCst);

    // Wake parent if it's blocked waiting for us
    // This puts the parent back in the ready queue
    if parent_pid > 0 {
        wake_parent(parent_pid);
        let mut writer = serial::SerialWriter;
        let _ = writeln!(writer, "[EXIT] Woke parent {}", parent_pid);
    }

    // Check if there's a saved parent context to return to (legacy path)
    let parent_ctx = PARENT_CONTEXT.lock().take();

    if let Some(ctx) = parent_ctx {
        // Restore parent as current process
        table.set_current_pid(ctx.pid as u32);

        // Get parent's kernel stack
        if let Some(parent) = table.get(ctx.pid as u32) {
            let p = parent.lock();
            let parent_stack_phys = p.kernel_stack();
            let parent_stack_size = p.kernel_stack_size();
            drop(p);

            let parent_stack_virt = phys_to_virt(parent_stack_phys);
            let parent_stack_top = parent_stack_virt.as_u64() + parent_stack_size as u64;

            // Restore parent's kernel stack for syscalls
            unsafe {
                arch::syscall::set_kernel_stack(parent_stack_top);
            }
            arch::gdt::set_kernel_stack(parent_stack_top);

            // Calculate wait result: (child_pid << 32) | status
            let wait_result = ((current_pid as i64) << 32) | ((status as i64) & 0xFFFFFFFF);

            // Return to parent's user mode via sysretq
            // CRITICAL: Must restore ALL registers the parent had when making the waitpid syscall
            // sysretq clobbers RCX (uses for RIP) and R11 (uses for RFLAGS)
            // All other registers must be restored to parent's values

            // Copy context to static memory that survives the CR3 switch
            // We use a static because inline asm can't handle this many registers
            static mut RESTORE_CTX: ParentContext = ParentContext {
                pid: 0,
                pml4: 0,
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
            };
            static mut RESTORE_RESULT: i64 = 0;

            unsafe {
                use core::ptr::addr_of_mut;
                *addr_of_mut!(RESTORE_CTX) = ctx.clone();
                *addr_of_mut!(RESTORE_RESULT) = wait_result;

                // Switch page tables first
                core::arch::asm!(
                    "mov cr3, {}",
                    in(reg) ctx.pml4,
                    options(nostack)
                );

                // Now restore all registers from the static context and sysretq
                // The context is at a fixed virtual address (higher half)
                let ctx_ptr = addr_of_mut!(RESTORE_CTX) as u64;
                let _result_ptr = addr_of_mut!(RESTORE_RESULT) as u64;

                // ParentContext layout:
                // pid: u32 (offset 0, padded to 8)
                // pml4: u64 (offset 8)
                // rip: u64 (offset 16)
                // rsp: u64 (offset 24)
                // rflags: u64 (offset 32)
                // rax: u64 (offset 40)
                // rbx: u64 (offset 48)
                // rcx: u64 (offset 56)
                // rdx: u64 (offset 64)
                // rsi: u64 (offset 72)
                // rdi: u64 (offset 80)
                // rbp: u64 (offset 88)
                // r8: u64 (offset 96)
                // r9: u64 (offset 104)
                // r10: u64 (offset 112)
                // r11: u64 (offset 120)
                // r12: u64 (offset 128)
                // r13: u64 (offset 136)
                // r14: u64 (offset 144)
                // r15: u64 (offset 152)
                // Store values in statics so we can access them after restoring all registers
                static mut SYSRET_USER_RSP: u64 = 0;
                static mut SYSRET_USER_RIP: u64 = 0;
                static mut SYSRET_USER_RFLAGS: u64 = 0;
                static mut SYSRET_RESULT: i64 = 0;
                static mut SYSRET_R14: u64 = 0;
                static mut SYSRET_R15: u64 = 0;

                *addr_of_mut!(SYSRET_USER_RSP) = ctx.rsp;
                *addr_of_mut!(SYSRET_USER_RIP) = ctx.rip;
                *addr_of_mut!(SYSRET_USER_RFLAGS) = ctx.rflags;
                *addr_of_mut!(SYSRET_RESULT) = wait_result;
                *addr_of_mut!(SYSRET_R14) = ctx.r14;
                *addr_of_mut!(SYSRET_R15) = ctx.r15;

                core::arch::asm!(
                    // r15 = context pointer (only used for loading registers, not for sysret values)
                    "mov r15, {ctx}",
                    // Restore callee-saved registers
                    "mov rbx, [r15 + 48]",    // rbx at offset 48
                    "mov rbp, [r15 + 88]",    // rbp at offset 88
                    "mov r12, [r15 + 128]",   // r12 at offset 128
                    "mov r13, [r15 + 136]",   // r13 at offset 136
                    // Restore caller-saved registers (that syscall should preserve)
                    "mov rdi, [r15 + 80]",    // rdi at offset 80
                    "mov rsi, [r15 + 72]",    // rsi at offset 72
                    "mov rdx, [r15 + 64]",    // rdx at offset 64
                    "mov r8, [r15 + 96]",     // r8 at offset 96
                    "mov r9, [r15 + 104]",    // r9 at offset 104
                    "mov r10, [r15 + 112]",   // r10 at offset 112
                    // Now load sysret values and r14/r15 from statics (using absolute addresses)
                    "mov rax, [{result}]",    // result value
                    "mov rcx, [{rip}]",       // user rip
                    "mov r11, [{rflags}]",    // user rflags
                    "mov r14, [{r14_val}]",   // restore r14
                    "mov r15, [{r15_val}]",   // restore r15
                    // Load user RSP last and sysretq
                    "mov rsp, [{rsp_val}]",
                    "sysretq",
                    ctx = in(reg) ctx_ptr,
                    result = sym SYSRET_RESULT,
                    rip = sym SYSRET_USER_RIP,
                    rflags = sym SYSRET_USER_RFLAGS,
                    r14_val = sym SYSRET_R14,
                    r15_val = sym SYSRET_R15,
                    rsp_val = sym SYSRET_USER_RSP,
                    options(noreturn)
                );
            }
        }
    }

    // Process exited - switch to next ready process
    // We can't rely on timer preemption here since we're in kernel mode
    // Must actively switch to another process
    {
        let mut writer = serial::SerialWriter;
        let _ = writeln!(writer, "[EXIT] Looking for next process (parent={})...", parent_pid);
    }

    // Remove exiting process from scheduler
    crate::scheduler::remove_process(current_pid);

    loop {
        // Prioritize switching to the parent if it's ready (it was just woken)
        let next_pid = if parent_pid > 0 {
            if let Some(parent) = table.get(parent_pid) {
                let state = parent.lock().state();
                if state == proc_traits::ProcessState::Ready
                    || state == proc_traits::ProcessState::Running {
                    Some(parent_pid)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        // Fall back to finding any ready process if parent isn't ready
        let next_pid = next_pid.or_else(|| {
            let all_pids = table.all_pids();
            all_pids.iter().find_map(|&pid| {
                if pid == current_pid {
                    return None;
                }
                table.get(pid).and_then(|proc_arc| {
                    let proc = proc_arc.lock();
                    if proc.state() == proc_traits::ProcessState::Ready
                        || proc.state() == proc_traits::ProcessState::Running
                    {
                        Some(pid)
                    } else {
                        None
                    }
                })
            })
        });

        let next_pid = match next_pid {
            Some(pid) => pid,
            None => {
                // No other processes - halt the system
                {
                    let mut writer = serial::SerialWriter;
                    let _ = writeln!(writer, "[EXIT] No ready processes! System halting.");
                }
                arch::X86_64::halt();
            }
        };

        // Get next process info from scheduler's Task (not Process.context!)
        // This is critical: the scheduler updates Task.context on preemption,
        // but Process.context may be stale (e.g., from fork() time).
        let (next_ctx, next_pml4, kernel_stack_top) = {
            // First try to get context from scheduler's Task
            if let Some((task_ctx, pml4, ks_phys, ks_size)) = sched::get_task_switch_info(next_pid) {
                let ks_top = phys_to_virt(ks_phys).as_u64() + ks_size as u64;
                // Convert TaskContext to ProcessContext
                let ctx = ProcessContext {
                    rip: task_ctx.rip,
                    rsp: task_ctx.rsp,
                    rflags: task_ctx.rflags,
                    rax: task_ctx.rax,
                    rbx: task_ctx.rbx,
                    rcx: task_ctx.rcx,
                    rdx: task_ctx.rdx,
                    rsi: task_ctx.rsi,
                    rdi: task_ctx.rdi,
                    rbp: task_ctx.rbp,
                    r8: task_ctx.r8,
                    r9: task_ctx.r9,
                    r10: task_ctx.r10,
                    r11: task_ctx.r11,
                    r12: task_ctx.r12,
                    r13: task_ctx.r13,
                    r14: task_ctx.r14,
                    r15: task_ctx.r15,
                    cs: task_ctx.cs,
                    ss: task_ctx.ss,
                };
                (ctx, pml4, ks_top)
            } else {
                // Fallback to Process.context if task not in scheduler
                let next = match table.get(next_pid) {
                    Some(p) => p,
                    None => continue, // Process gone, try next
                };
                let proc = next.lock();
                (proc.context().clone(), proc.address_space().pml4_phys(), {
                    let ks_virt = phys_to_virt(proc.kernel_stack());
                    ks_virt.as_u64() + proc.kernel_stack_size() as u64
                })
            }
        };

        // Debug: print the context we're about to restore
        {
            let mut writer = serial::SerialWriter;
            let _ = writeln!(writer, "[EXIT] Switching to PID {}", next_pid);
            let _ = writeln!(writer, "[EXIT] Context: rip={:#x} rsp={:#x} cs={:#x} ss={:#x}",
                next_ctx.rip, next_ctx.rsp, next_ctx.cs, next_ctx.ss);
        }

        // Switch to next process
        table.set_current_pid(next_pid);

        // Update kernel stack pointers
        unsafe {
            arch::syscall::set_kernel_stack(kernel_stack_top);
        }
        arch::gdt::set_kernel_stack(kernel_stack_top);

        // Use same context switch mechanism as kernel_yield
        static mut EXIT_CTX: ProcessContext = ProcessContext {
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
            *addr_of_mut!(EXIT_CTX) = next_ctx.clone();

            // Switch page tables
            core::arch::asm!("mov cr3, {}", in(reg) next_pml4.as_u64());

            let ctx_ptr = addr_of_mut!(EXIT_CTX) as u64;

            // Check if returning to user mode or kernel mode
            if next_ctx.cs == 0x23 {
                // User mode - use sysretq
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
                    "push qword ptr [rax + 8]",  // Push user rsp
                    "mov rax, [rax + 24]",       // Load return value
                    "pop rsp",
                    "swapgs",
                    "sysretq",
                    ctx = in(reg) ctx_ptr,
                    options(noreturn)
                );
            } else {
                // Kernel mode - use iretq
                // Build iretq frame on stack: SS, RSP, RFLAGS, CS, RIP
                core::arch::asm!(
                    "mov rax, {ctx}",
                    // Load all GP registers first
                    "mov rbx, [rax + 32]",
                    "mov rcx, [rax + 40]",
                    "mov rdx, [rax + 48]",
                    "mov rsi, [rax + 56]",
                    "mov rdi, [rax + 64]",
                    "mov rbp, [rax + 72]",
                    "mov r8, [rax + 80]",
                    "mov r9, [rax + 88]",
                    "mov r10, [rax + 96]",
                    "mov r11, [rax + 104]",
                    "mov r12, [rax + 112]",
                    "mov r13, [rax + 120]",
                    "mov r14, [rax + 128]",
                    "mov r15, [rax + 136]",
                    // Build iretq frame (push in reverse order)
                    "push qword ptr [rax + 152]", // SS (offset of ss in ProcessContext)
                    "push qword ptr [rax + 8]",   // RSP
                    "mov r11, [rax + 16]",        // RFLAGS
                    "or r11, 0x200",              // Ensure IF set
                    "push r11",
                    "push qword ptr [rax + 144]", // CS (offset of cs in ProcessContext)
                    "push qword ptr [rax]",       // RIP
                    // Load rax last
                    "mov rax, [rax + 24]",
                    // No swapgs for kernel mode
                    "iretq",
                    ctx = in(reg) ctx_ptr,
                    options(noreturn)
                );
            }
        }
    }
}

/// Fork callback for syscalls
///
/// Creates a child process and returns child PID to parent, 0 to child.
pub fn kernel_fork() -> i64 {
    let table = process_table();
    let parent_pid = table.current_pid();

    debug_fork!("[FORK] Fork called from PID {}", parent_pid);

    // Get current process context from syscall user context
    let user_ctx = arch::get_user_context();

    debug_fork!(
        "[FORK] user_ctx.rip={:#x} rsp={:#x}",
        user_ctx.rip,
        user_ctx.rsp
    );
    let parent_context = ProcessContext {
        rip: user_ctx.rip,
        rsp: user_ctx.rsp,
        rflags: user_ctx.rflags,
        rax: user_ctx.rax,
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

    debug_fork!(
        "[FORK] Parent context: rip={:#x} rsp={:#x}",
        parent_context.rip,
        parent_context.rsp
    );

    // Create wrapper for frame allocator
    let alloc_wrapper = FrameAllocatorWrapper;

    // Call do_fork
    let result = do_fork(parent_pid, &parent_context, &alloc_wrapper);

    match result {
        Ok(child_pid) => {
            debug_fork!("[FORK] Created child process {}", child_pid);

            // Save parent context with fork return value (child_pid)
            // Then switch to child immediately - child runs first
            let parent_task_ctx = TaskContext {
                rip: parent_context.rip,
                rsp: parent_context.rsp,
                rflags: parent_context.rflags,
                rax: child_pid as u64, // Parent's fork() returns child_pid
                rbx: parent_context.rbx,
                rcx: parent_context.rcx,
                rdx: parent_context.rdx,
                rsi: parent_context.rsi,
                rdi: parent_context.rdi,
                rbp: parent_context.rbp,
                r8: parent_context.r8,
                r9: parent_context.r9,
                r10: parent_context.r10,
                r11: parent_context.r11,
                r12: parent_context.r12,
                r13: parent_context.r13,
                r14: parent_context.r14,
                r15: parent_context.r15,
                cs: 0x23, // User mode
                ss: 0x1B,
            };

            // Update parent's context in the scheduler's Task
            // This is critical - the scheduler uses Task.context for context switching
            sched::set_task_context(parent_pid, parent_task_ctx);

            // Also update Process.context for backward compatibility
            if let Some(parent) = table.get(parent_pid) {
                let mut proc = parent.lock();
                let ctx = proc.context_mut();
                ctx.rip = parent_task_ctx.rip;
                ctx.rsp = parent_task_ctx.rsp;
                ctx.rflags = parent_task_ctx.rflags;
                ctx.rax = parent_task_ctx.rax;
                ctx.rbx = parent_task_ctx.rbx;
                ctx.rcx = parent_task_ctx.rcx;
                ctx.rdx = parent_task_ctx.rdx;
                ctx.rsi = parent_task_ctx.rsi;
                ctx.rdi = parent_task_ctx.rdi;
                ctx.rbp = parent_task_ctx.rbp;
                ctx.r8 = parent_task_ctx.r8;
                ctx.r9 = parent_task_ctx.r9;
                ctx.r10 = parent_task_ctx.r10;
                ctx.r11 = parent_task_ctx.r11;
                ctx.r12 = parent_task_ctx.r12;
                ctx.r13 = parent_task_ctx.r13;
                ctx.r14 = parent_task_ctx.r14;
                ctx.r15 = parent_task_ctx.r15;
                ctx.cs = parent_task_ctx.cs;
                ctx.ss = parent_task_ctx.ss;
            }

            // Add child process to scheduler
            add_process(child_pid);

            // Tell scheduler we're switching from parent to child
            // This re-enqueues the parent so it can run later
            sched::switch_to(child_pid);

            // Get child's context and switch to it
            let (child_ctx, child_pml4, child_kstack_top) = {
                let child = match table.get(child_pid) {
                    Some(c) => c,
                    None => return -1,
                };
                let proc = child.lock();
                (proc.context().clone(), proc.address_space().pml4_phys(), {
                    let ks_virt = phys_to_virt(proc.kernel_stack());
                    ks_virt.as_u64() + proc.kernel_stack_size() as u64
                })
            };

            // Switch to child process
            table.set_current_pid(child_pid);

            // Update kernel stack
            unsafe {
                arch::syscall::set_kernel_stack(child_kstack_top);
            }
            arch::gdt::set_kernel_stack(child_kstack_top);

            // Switch to child via sysretq (child's fork returns 0)
            static mut FORK_CHILD_CTX: ProcessContext = ProcessContext {
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
                *addr_of_mut!(FORK_CHILD_CTX) = child_ctx;

                // Switch page tables
                core::arch::asm!("mov cr3, {}", in(reg) child_pml4.as_u64());

                let ctx_ptr = addr_of_mut!(FORK_CHILD_CTX) as u64;

                // Child's fork() returns 0 (already set in do_fork)
                core::arch::asm!(
                    "mov rax, {ctx}",
                    "mov rcx, [rax]",       // rip
                    "mov r11, [rax + 16]",  // rflags
                    "or r11, 0x200",        // Ensure IF
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
                    "mov rax, [rax + 24]",  // rax = 0 (child return)
                    "pop rsp",
                    "swapgs",
                    "sysretq",
                    ctx = in(reg) ctx_ptr,
                    options(noreturn)
                );
            }
        }
        Err(_e) => {
            debug_fork!("[FORK] Fork failed: {:?}", _e);
            -1 // EAGAIN
        }
    }
}

/// Wait callback for syscalls
///
/// Waits for child process and returns (pid << 32) | status.
/// Properly blocks until a child exits (unless WNOHANG is set).
pub fn kernel_wait(pid: i32, options: i32) -> i64 {
    let table = process_table();
    let parent_pid = table.current_pid();
    let wait_opts = WaitOptions::from(options);

    loop {
        // Check for zombie children
        match do_waitpid(parent_pid, pid, wait_opts) {
            Ok(result) => {
                // Debug: print framebuffer write stats after each child exits
                {
                    let (writes, bytes, base) = devfs::devices::get_fb_write_stats();
                    let mut writer = serial::SerialWriter;
                    let _ = writeln!(
                        writer,
                        "[FB_DEBUG] Child {} exited - FB writes={} bytes={} base={:#x}",
                        result.pid, writes, bytes, base
                    );
                }
                // Pack pid and status into result
                return ((result.pid as i64) << 32) | ((result.status as i64) & 0xFFFFFFFF);
            }
            Err(e) => {
                match e {
                    proc::WaitError::NoChildren => return -10, // ECHILD
                    proc::WaitError::InvalidPid => return -3,  // ESRCH
                    proc::WaitError::Interrupted => return -4, // EINTR
                    proc::WaitError::WouldBlock => {
                        // If WNOHANG, return immediately
                        if wait_opts.nohang {
                            return 0; // No child exited yet
                        }

                        // Mark this process as waiting for the child
                        // This allows user_exit to find us and wake us up
                        if let Some(proc) = table.get(parent_pid) {
                            proc.lock().wait_for_child(pid);
                        }

                        // Allow scheduler to preempt us while we wait
                        arch::allow_kernel_preempt();

                        // Wait for timer interrupt - scheduler will run other processes
                        // When child exits, wake_parent() will set us to Ready
                        unsafe {
                            core::arch::asm!("sti"); // Ensure interrupts enabled
                            core::arch::asm!("hlt", options(nomem, nostack));
                        }

                        // Clear preempt flag if we're still running
                        arch::disallow_kernel_preempt();
                        continue;
                    }
                }
            }
        }
    }
}

/// Run a child process to completion
///
/// This function saves the parent's context and enters the child.
/// When the child exits, control returns to parent via sysretq.
#[allow(dead_code)]
pub fn run_child_process(child_pid: Pid) {
    let table = process_table();
    let parent_pid = table.current_pid();

    // Get parent's PML4 for restoring later
    let _parent_pml4 = if let Some(p) = table.get(parent_pid) {
        p.lock().address_space().pml4_phys().as_u64()
    } else {
        // Fallback to kernel PML4
        unsafe { KERNEL_PML4 }
    };

    // Get child process info
    let (child_pml4, _child_entry, _child_stack, kernel_stack_phys, kernel_stack_size) = {
        let child = match table.get(child_pid) {
            Some(c) => c,
            None => return,
        };

        let c = child.lock();
        (
            c.address_space().pml4_phys(),
            c.entry_point(),
            c.user_stack_top(),
            c.kernel_stack(),
            c.kernel_stack_size(),
        )
    };

    // Set current process to child
    table.set_current_pid(child_pid);
    #[cfg(feature = "debug-fork")]
    {
        let verify_pid = table.current_pid();
        debug_fork!(
            "[RUN_CHILD] set_current_pid({}) done, verify={}",
            child_pid,
            verify_pid
        );
    }

    // Use the kernel stack already allocated for this child (in fork)
    let kernel_stack_virt = phys_to_virt(kernel_stack_phys);
    let child_kernel_stack_top = kernel_stack_virt.as_u64() + kernel_stack_size as u64;

    // Set kernel stack for child's syscalls/interrupts
    unsafe {
        arch::syscall::set_kernel_stack(child_kernel_stack_top);
    }
    arch::gdt::set_kernel_stack(child_kernel_stack_top);

    // Get child's saved context
    let child_ctx = {
        let child = table.get(child_pid).unwrap();
        child.lock().context().clone()
    };

    // Debug: print child's context (all callee-saved registers)
    debug_fork!("[CHILD] PID {} entering usermode", child_pid);
    debug_fork!(
        "[CHILD] rip={:#x} rsp={:#x} rbp={:#x}",
        child_ctx.rip,
        child_ctx.rsp,
        child_ctx.rbp
    );
    debug_fork!(
        "[CHILD] rax={:#x} rbx={:#x} r12={:#x}",
        child_ctx.rax,
        child_ctx.rbx,
        child_ctx.r12
    );
    debug_fork!(
        "[CHILD] r13={:#x} r14={:#x} r15={:#x}",
        child_ctx.r13,
        child_ctx.r14,
        child_ctx.r15
    );

    // Save parent's FULL user context so we can restore ALL registers when child exits
    // This is critical because the parent's syscall handler saved registers to the
    // kernel stack, but we're going to bypass the normal epilogue via user_exit's sysretq
    let parent_user_ctx = arch::get_user_context();
    {
        *PARENT_CONTEXT.lock() = Some(ParentContext {
            pid: parent_pid as u64,
            pml4: _parent_pml4,
            rip: parent_user_ctx.rip,
            rsp: parent_user_ctx.rsp,
            rflags: parent_user_ctx.rflags,
            rax: parent_user_ctx.rax,
            rbx: parent_user_ctx.rbx,
            rcx: parent_user_ctx.rcx,
            rdx: parent_user_ctx.rdx,
            rsi: parent_user_ctx.rsi,
            rdi: parent_user_ctx.rdi,
            rbp: parent_user_ctx.rbp,
            r8: parent_user_ctx.r8,
            r9: parent_user_ctx.r9,
            r10: parent_user_ctx.r10,
            r11: parent_user_ctx.r11,
            r12: parent_user_ctx.r12,
            r13: parent_user_ctx.r13,
            r14: parent_user_ctx.r14,
            r15: parent_user_ctx.r15,
        });
        CHILD_DONE.store(false, Ordering::SeqCst);
    }

    // Build UserContext for enter_usermode_with_context
    let user_ctx = arch::UserContext {
        rax: child_ctx.rax,
        rbx: child_ctx.rbx,
        rcx: child_ctx.rcx,
        rdx: child_ctx.rdx,
        rsi: child_ctx.rsi,
        rdi: child_ctx.rdi,
        rbp: child_ctx.rbp,
        rsp: child_ctx.rsp,
        r8: child_ctx.r8,
        r9: child_ctx.r9,
        r10: child_ctx.r10,
        r11: child_ctx.r11,
        r12: child_ctx.r12,
        r13: child_ctx.r13,
        r14: child_ctx.r14,
        r15: child_ctx.r15,
        rip: child_ctx.rip,
        rflags: child_ctx.rflags,
    };

    // Debug: verify UserContext before entering usermode
    #[cfg(feature = "debug-fork")]
    {
        debug_fork!("[CHILD] UserContext ptr: {:p}", &user_ctx);
        debug_fork!(
            "[CHILD] UserContext.rip={:#x} rsp={:#x}",
            user_ctx.rip,
            user_ctx.rsp
        );
        debug_fork!(
            "[CHILD] UserContext.rcx={:#x} rax={:#x}",
            user_ctx.rcx,
            user_ctx.rax
        );
        debug_fork!(
            "[CHILD] kernel_stack={:#x} pml4={:#x}",
            child_kernel_stack_top,
            child_pml4.as_u64()
        );

        // Verify by reading raw bytes at context address
        let ctx_ptr = &user_ctx as *const arch::UserContext as *const u64;
        unsafe {
            debug_fork!("[CHILD] Raw ctx[0]={:#x} (rax)", *ctx_ptr.add(0));
            debug_fork!("[CHILD] Raw ctx[2]={:#x} (rcx)", *ctx_ptr.add(2));
            debug_fork!("[CHILD] Raw ctx[16]={:#x} (rip)", *ctx_ptr.add(16));
        }

        // Test: copy context to child kernel stack and verify it's readable after CR3 switch
        // Use EXACT same address as enter_usermode_with_context: kernel_stack_top - 184
        let child_stack_base = child_kernel_stack_top - 184;
        let dest_ptr = child_stack_base as *mut u64;
        debug_fork!("[CHILD] Test dest_ptr={:#x}", dest_ptr as u64);
        debug_fork!("[CHILD] rcx will be at {:#x}", dest_ptr as u64 + 16);
        let src_ptr = &user_ctx as *const arch::UserContext as *const u64;

        // Copy context to child's kernel stack
        for i in 0..18 {
            unsafe {
                *dest_ptr.add(i) = *src_ptr.add(i);
            }
        }

        // Now switch to child's page tables and read back
        unsafe {
            // Read CR3 to verify current value
            let current_cr3: u64;
            core::arch::asm!("mov {}, cr3", out(reg) current_cr3);
            debug_fork!("[CHILD] Current CR3: {:#x}", current_cr3);
            debug_fork!("[CHILD] Child PML4: {:#x}", child_pml4.as_u64());

            // Switch to child's page tables
            core::arch::asm!("mov cr3, {}", in(reg) child_pml4.as_u64());

            // Read back from the copied context
            let read_rax = *dest_ptr.add(0);
            let read_rcx = *dest_ptr.add(2);
            let read_rip = *dest_ptr.add(16);

            // Switch back to original page tables
            core::arch::asm!("mov cr3, {}", in(reg) current_cr3);

            debug_fork!("[CHILD] After CR3 switch and back:");
            debug_fork!("[CHILD]   read_rax={:#x}", read_rax);
            debug_fork!("[CHILD]   read_rcx={:#x}", read_rcx);
            debug_fork!("[CHILD]   read_rip={:#x}", read_rip);
        }
    }

    // Enter user mode for child with full context restoration
    // When child calls exit(), user_exit will set CHILD_DONE and we'll detect it
    unsafe {
        arch::enter_usermode_with_context(child_kernel_stack_top, child_pml4.as_u64(), &user_ctx);
    }

    // Note: We never reach here via normal flow.
    // But if we did somehow return, that would be the child exit path.
}

/// Exec callback for syscalls
///
/// Replaces the current process image with a new executable.
pub fn kernel_exec(
    path_ptr: *const u8,
    path_len: usize,
    argv_ptr: *const *const u8,
    envp_ptr: *const *const u8,
) -> i64 {
    let table = process_table();
    let current_pid = table.current_pid();

    // Read path from user space
    let path = unsafe {
        if path_ptr.is_null() || path_len == 0 {
            debug_fork!("[EXEC] Invalid path (null or zero len)");
            return -22; // EINVAL
        }
        let slice = core::slice::from_raw_parts(path_ptr, path_len);
        match core::str::from_utf8(slice) {
            Ok(s) => s,
            Err(_) => {
                debug_fork!("[EXEC] Invalid UTF-8 in path");
                return -22; // EINVAL
            }
        }
    };

    debug_fork!("[EXEC] PID {} exec(\"{}\")", current_pid, path);

    // Read argv from user space
    let mut argv: Vec<String> = Vec::new();
    if !argv_ptr.is_null() {
        unsafe {
            let mut i = 0;
            loop {
                let arg_ptr = *argv_ptr.add(i);
                if arg_ptr.is_null() {
                    break;
                }
                // Validate pointer is in user space before touching
                let arg_addr = arg_ptr as u64;
                if arg_addr == 0 || arg_addr >= 0x0000_8000_0000_0000 {
                    debug_fork!(
                        "[EXEC] argv[{i}] pointer out of user space: {:#x}",
                        arg_addr
                    );
                    return -14; // EFAULT
                }
                // Read null-terminated string with a hard cap
                let mut len = 0;
                while len < 4096 {
                    let ch = *arg_ptr.add(len);
                    if ch == 0 {
                        break;
                    }
                    len += 1;
                }
                if len == 4096 {
                    debug_fork!("[EXEC] argv[{i}] exceeds 4096 bytes");
                    return -22; // EINVAL
                }
                let arg_slice = core::slice::from_raw_parts(arg_ptr, len);
                match core::str::from_utf8(arg_slice) {
                    Ok(s) => argv.push(String::from(s)),
                    Err(_) => {
                        debug_fork!("[EXEC] argv[{i}] invalid UTF-8");
                        return -22; // EINVAL
                    }
                }
                i += 1;
                if i > 1024 {
                    debug_fork!("[EXEC] argv too long (>1024)");
                    return -22; // EINVAL
                }
            }
        }
    }
    // If no argv provided, use the path as argv[0]
    if argv.is_empty() {
        argv.push(String::from(path));
    }

    // Read envp from user space
    let mut envp: Vec<String> = Vec::new();
    if !envp_ptr.is_null() {
        unsafe {
            let mut i = 0;
            loop {
                let env_ptr = *envp_ptr.add(i);
                if env_ptr.is_null() {
                    break;
                }
                let env_addr = env_ptr as u64;
                if env_addr == 0 || env_addr >= 0x0000_8000_0000_0000 {
                    debug_fork!(
                        "[EXEC] envp[{i}] pointer out of user space: {:#x}",
                        env_addr
                    );
                    return -14; // EFAULT
                }
                // Read null-terminated string with a hard cap
                let mut len = 0;
                while len < 4096 {
                    let ch = *env_ptr.add(len);
                    if ch == 0 {
                        break;
                    }
                    len += 1;
                }
                if len == 4096 {
                    debug_fork!("[EXEC] envp[{i}] exceeds 4096 bytes");
                    return -22; // EINVAL
                }
                let env_slice = core::slice::from_raw_parts(env_ptr, len);
                match core::str::from_utf8(env_slice) {
                    Ok(s) => envp.push(String::from(s)),
                    Err(_) => {
                        debug_fork!("[EXEC] envp[{i}] invalid UTF-8");
                        return -22; // EINVAL
                    }
                }
                i += 1;
                if i > 1024 {
                    debug_fork!("[EXEC] envp too long (>1024)");
                    return -22; // EINVAL
                }
            }
        }
    }

    // Look up the file in VFS, following symlinks
    let mut vnode = match GLOBAL_VFS.lookup(path) {
        Ok(v) => v,
        Err(_e) => {
            debug_fork!("[EXEC] VFS lookup failed for '{}': {:?}", path, _e);
            return -2; // ENOENT
        }
    };

    // Follow symlinks (up to 40 levels to prevent infinite loops)
    let mut symlink_count = 0;
    while vnode.vtype() == VnodeType::Symlink {
        symlink_count += 1;
        if symlink_count > 40 {
            debug_fork!("[EXEC] Too many symlinks (>40)");
            return -40; // ELOOP
        }

        // Read the symlink target
        let target = match vnode.readlink() {
            Ok(t) => t,
            Err(_e) => {
                debug_fork!("[EXEC] Failed to read symlink: {:?}", _e);
                return -2; // ENOENT
            }
        };

        // Resolve the target (could be absolute or relative)
        let resolved_path = if target.starts_with('/') {
            target
        } else {
            // Relative symlink - get parent directory of current path
            let parent = path.rfind('/').map(|i| &path[..i]).unwrap_or("/");
            alloc::format!("{}/{}", parent, target)
        };

        // Look up the target
        vnode = match GLOBAL_VFS.lookup(&resolved_path) {
            Ok(v) => v,
            Err(_e) => {
                debug_fork!(
                    "[EXEC] Symlink target lookup failed for '{}': {:?}",
                    resolved_path,
                    _e
                );
                return -2; // ENOENT
            }
        };
    }

    // Check it's a regular file
    if vnode.vtype() != VnodeType::File {
        debug_fork!("[EXEC] Not a regular file: {:?}", vnode.vtype());
        return -21; // EISDIR or not a file
    }

    // Read the file contents
    let size = vnode.size() as usize;
    debug_fork!("[EXEC] File size: {} bytes", size);

    let mut elf_data = alloc::vec![0u8; size];
    let read_result = vnode.read(0, &mut elf_data);
    let bytes_read = match read_result {
        Ok(n) => n,
        Err(_e) => {
            debug_fork!("[EXEC] Read failed: {:?}", _e);
            return -5; // EIO
        }
    };

    if bytes_read != size {
        debug_fork!("[EXEC] Short read: {} of {} bytes", bytes_read, size);
        return -5; // EIO - short read
    }
    debug_fork!("[EXEC] Read {} bytes, calling do_exec", bytes_read);

    // Get kernel PML4 for creating new address space
    let kernel_pml4 = PhysAddr::new(unsafe { KERNEL_PML4 });
    let alloc_wrapper = FrameAllocatorWrapper;

    // Call do_exec
    match do_exec(
        current_pid,
        &elf_data,
        &argv,
        &envp,
        &alloc_wrapper,
        kernel_pml4,
    ) {
        Ok((_entry_point, _stack_ptr)) => {
            // Get the new PML4 and switch to it
            if let Some(proc) = table.get(current_pid) {
                let p = proc.lock();
                let new_pml4 = p.address_space().pml4_phys();
                let ctx = p.context().clone();
                drop(p);

                // Debug: print exec return values
                debug_fork!("[EXEC] Switching to PML4={:#x}", new_pml4.as_u64());
                debug_fork!("[EXEC] rip={:#x} rsp={:#x}", ctx.rip, ctx.rsp);
                debug_fork!(
                    "[EXEC] argc={} argv={:#x} envp={:#x}",
                    ctx.rdi,
                    ctx.rsi,
                    ctx.rdx
                );

                // Switch to new address space and jump to entry point
                unsafe {
                    write_cr3(new_pml4);
                    flush_tlb_all();

                    // Return to user mode at new entry point
                    // We use sysretq which expects: rcx = rip, r11 = rflags
                    // Use explicit registers to prevent compiler from reusing registers
                    // that we overwrite before their values are consumed
                    core::arch::asm!(
                        // Set up rip for sysretq
                        "mov rcx, r8",
                        // Set up rflags for sysretq
                        "mov r11, r9",
                        // Set up user stack - do this AFTER loading values into rcx/r11
                        // to avoid any chance of compiler putting inputs in rsp
                        "mov rsp, r10",
                        // Set up argc, argv, envp in registers per System V ABI
                        // These are already loaded into r12, r13, r14 respectively
                        "mov rdi, r12",
                        "mov rsi, r13",
                        "mov rdx, r14",
                        // Load user data segment selectors for DS/ES/FS (0x1B = USER_DS | 3)
                        // NOTE: Do NOT load GS - swapgs will handle it
                        "mov ax, 0x1b",
                        "mov ds, ax",
                        "mov es, ax",
                        "mov fs, ax",
                        // Clear rax for return value
                        "xor rax, rax",
                        // Swap GS back to user mode (required before sysretq)
                        "swapgs",
                        // Return to user mode
                        "sysretq",
                        in("r8") ctx.rip,
                        in("r9") 0x202u64, // IF set
                        in("r10") ctx.rsp,
                        in("r12") ctx.rdi,
                        in("r13") ctx.rsi,
                        in("r14") ctx.rdx,
                        options(noreturn)
                    );
                }
            }
            0 // Never reached
        }
        Err(e) => {
            let code = match e {
                proc::ExecError::InvalidElf => {
                    debug_fork!("[EXEC] Error: InvalidElf");
                    -8 // ENOEXEC
                }
                proc::ExecError::OutOfMemory => {
                    debug_fork!("[EXEC] Error: OutOfMemory");
                    -12 // ENOMEM
                }
                proc::ExecError::ProcessNotFound => {
                    debug_fork!("[EXEC] Error: ProcessNotFound");
                    -3 // ESRCH
                }
                proc::ExecError::InvalidAddress => {
                    debug_fork!("[EXEC] Error: InvalidAddress");
                    -14 // EFAULT
                }
                proc::ExecError::InvalidArgument => {
                    debug_fork!("[EXEC] Error: InvalidArgument");
                    -22 // EINVAL
                }
            };
            code
        }
    }
}
