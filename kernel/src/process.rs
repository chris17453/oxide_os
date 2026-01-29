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
use proc::{ProcessContext, ProcessMeta, WaitOptions, WaitResult, do_exec, do_fork};
use proc_traits::Pid;
use sched::TaskState;
use vfs::{VnodeType, mount::GLOBAL_VFS};

#[allow(unused_imports)]
use crate::debug_fork;
use crate::globals::{
    CHILD_DONE, KERNEL_PML4, PARENT_CONTEXT, ParentContext, USER_EXIT_STATUS, USER_EXITED,
};
use mm_manager::mm;
use crate::scheduler::{add_process, wake_parent};
use sched::TaskContext;

/// User exit function
pub fn user_exit(status: i32) -> ! {
    // Get current process and mark as zombie
    let current_pid = sched::current_pid().unwrap_or(0);

    // Get parent PID via scheduler
    let parent_pid = sched::get_task_ppid(current_pid).unwrap_or(0);

    // Mark task as zombie and set exit status via scheduler
    sched::set_task_exit_status(current_pid, status);

    unsafe {
        USER_EXIT_STATUS = status;
    }
    USER_EXITED.store(true, Ordering::SeqCst);

    // Wake parent if it's blocked waiting for us
    // This puts the parent back in the ready queue
    if parent_pid > 0 {
        wake_parent(parent_pid);
    }

    // Clear any stale PARENT_CONTEXT (we now use the scheduler for all task switches)
    let _ = PARENT_CONTEXT.lock().take();

    // NOTE: Do NOT remove the process from scheduler here!
    // The process must stay as a zombie until the parent reaps it via wait().
    // The parent's wait() call will remove it after getting the exit status.

    // Let the scheduler handle the context switch via the timer interrupt.
    // We are now a zombie - the scheduler won't pick us to run again.
    // The timer interrupt uses iretq which correctly handles returning to
    // either kernel mode (parent blocked in waitpid/HLT) or user mode.
    // Using sysretq here would be WRONG because sysretq always returns to
    // user mode, but the parent may have been preempted in kernel mode.
    sched::set_need_resched();
    arch::allow_kernel_preempt();
    loop {
        unsafe {
            core::arch::asm!("sti", options(nomem, nostack, preserves_flags));
            core::arch::asm!("hlt", options(nomem, nostack));
        }
    }
}

/// Get current task's FS base register value
///
/// Returns the fs_base from the current task's context (for TLS)
pub fn get_current_task_fs_base() -> u64 {
    let current_pid = sched::current_pid().unwrap_or(0);
    if let Some(ctx) = sched::get_task_context(current_pid) {
        ctx.fs_base
    } else {
        0
    }
}

/// Fork callback for syscalls
///
/// Creates a child process and returns child PID to parent, 0 to child.
pub fn kernel_fork() -> i64 {
    use alloc::sync::Arc;
    use spin::Mutex;

    let parent_pid = sched::current_pid().unwrap_or(0);

    debug_fork!("[FORK] Fork called from PID {}", parent_pid);

    // Get parent's ProcessMeta from scheduler
    let parent_meta_arc = match sched::get_task_meta(parent_pid) {
        Some(m) => m,
        None => {
            debug_fork!("[FORK] Parent meta not found");
            return -3; // ESRCH
        }
    };

    
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
        fs_base: 0, // Will be set by exec if TLS is needed
    };

    debug_fork!(
        "[FORK] Parent context: rip={:#x} rsp={:#x}",
        parent_context.rip,
        parent_context.rsp
    );

    
    // Kernel stack size (128KB)
    const KERNEL_STACK_SIZE: usize = 128 * 1024;

    // Call do_fork with parent's ProcessMeta
    let parent_meta = parent_meta_arc.lock();
    let result = do_fork(
        parent_pid,
        &parent_meta,
        &parent_context,
        mm(),
        KERNEL_STACK_SIZE,
    );

        // Save parent's PML4 before releasing the lock - needed for PARENT_CONTEXT
    let parent_pml4 = parent_meta.address_space.pml4_phys();
    drop(parent_meta); // Release lock before switching

    match result {
        Ok(fork_result) => {
            let child_pid = fork_result.child_pid;
            debug_fork!("[FORK] Created child process {}", child_pid);

            // Save parent context with fork return value (child_pid)
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
                fs_base: parent_context.fs_base,
            };

            // Update parent's context in the scheduler's Task
            sched::set_task_context(parent_pid, parent_task_ctx);

            // Wrap child's ProcessMeta in Arc<Mutex<>>
            let child_meta_arc = Arc::new(Mutex::new(fork_result.child_meta));

            // Get child's PML4 and context
            let child_pml4 = child_meta_arc.lock().address_space.pml4_phys();
            let child_ctx = fork_result.child_context.clone();

            // Create child's TaskContext (rax=0 for child return)
            let child_task_ctx = TaskContext {
                rip: child_ctx.rip,
                rsp: child_ctx.rsp,
                rflags: child_ctx.rflags,
                rax: 0, // Child's fork() returns 0
                rbx: child_ctx.rbx,
                rcx: child_ctx.rcx,
                rdx: child_ctx.rdx,
                rsi: child_ctx.rsi,
                rdi: child_ctx.rdi,
                rbp: child_ctx.rbp,
                r8: child_ctx.r8,
                r9: child_ctx.r9,
                r10: child_ctx.r10,
                r11: child_ctx.r11,
                r12: child_ctx.r12,
                r13: child_ctx.r13,
                r14: child_ctx.r14,
                r15: child_ctx.r15,
                cs: 0x23,
                ss: 0x1B,
                fs_base: child_ctx.fs_base,
            };

            // Create Task for child
            let child_task = sched::Task::new_with_meta(
                child_pid,
                parent_pid,
                fork_result.kernel_stack_phys,
                fork_result.kernel_stack_size,
                child_pml4,
                child_ctx.rip,
                child_ctx.rsp,
                child_meta_arc,
            );

            
            // Add child to scheduler
            sched::add_task(child_task);
            sched::set_task_context(child_pid, child_task_ctx);

            
            // Add child to parent's children list
            sched::add_task_child(parent_pid, child_pid);

            
            // Tell scheduler we're switching to child
            sched::switch_to(child_pid);

            
            // Get child's kernel stack top
            let child_kstack_virt = phys_to_virt(fork_result.kernel_stack_phys);
            let child_kstack_top = child_kstack_virt.as_u64() + fork_result.kernel_stack_size as u64;

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
                fs_base: 0,
            };

            // Save parent context so user_exit can restore it when child exits
            *PARENT_CONTEXT.lock() = Some(ParentContext {
                pid: parent_pid as u64,
                pml4: parent_pml4.as_u64(),
                rip: user_ctx.rip,
                rsp: user_ctx.rsp,
                rflags: user_ctx.rflags,
                rax: child_pid as u64, // fork() returns child_pid to parent
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
            });
            CHILD_DONE.store(false, Ordering::SeqCst);

            
            unsafe {
                *addr_of_mut!(FORK_CHILD_CTX) = child_ctx;

                // Switch page tables
                core::arch::asm!("mov cr3, {}", in(reg) child_pml4.as_u64());

                let ctx_ptr = addr_of_mut!(FORK_CHILD_CTX) as u64;

                // Child's fork() returns 0
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
                    "mov rdi, [rax + 64]",
                    "mov rsi, [rax + 56]",
                    "mov rdx, [rax + 48]",
                    "mov r8, [rax + 80]",
                    "mov r9, [rax + 88]",
                    "mov r10, [rax + 96]",
                    // Set FS base MSR if fs_base is non-zero (TLS support)
                    "push rax",              // Save context pointer
                    "mov r15, [rax + 160]",  // fs_base (offset 160 in ProcessContext)
                    "test r15, r15",
                    "jz 3f",
                    "mov ecx, 0xC0000100",  // MSR IA32_FS_BASE
                    "mov rax, r15",
                    "mov rdx, r15",
                    "shr rdx, 32",
                    "wrmsr",
                    "3:",
                    "pop rax",               // Restore context pointer
                    // Load user segment selectors for DS/ES (0x1B = USER_DS | 3)
                    // NOTE: In x86-64 long mode, FS base comes from FS_BASE MSR, not segment descriptor.
                    // We do NOT load FS at all - just leave it as-is after WRMSR set the base.
                    // Save context pointer to r15 temporarily
                    "mov r15, rax",
                    "mov ax, 0x1b",
                    "mov ds, ax",
                    "mov es, ax",
                    // Do NOT touch FS - leave it alone after WRMSR
                    // Restore rax as context pointer
                    "mov rax, r15",
                    // Now load r15 from context and prepare for return
                    "mov r15, [rax + 136]",
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
    let parent_pid = sched::current_pid().unwrap_or(0);
    let wait_opts = WaitOptions::from(options);

    loop {
        // Check for zombie children via scheduler
        match find_zombie_child(parent_pid, pid) {
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

                // Reap the zombie - remove from scheduler
                crate::scheduler::remove_process(result.pid);

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

                        // Update scheduler's Task state - mark as waiting
                        sched::set_task_waiting(parent_pid, pid);

                        // Block the current task in the scheduler
                        sched::block_current(TaskState::TASK_INTERRUPTIBLE);

                        // Mark that we need a reschedule
                        sched::set_need_resched();

                        // Allow scheduler to preempt us while we wait
                        arch::allow_kernel_preempt();

                        // Wait for timer interrupt - scheduler will run other processes
                        // When child exits, wake_parent() will wake us up
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

/// Find a zombie child process
///
/// Returns WaitResult if a matching zombie is found, WaitError otherwise.
fn find_zombie_child(parent_pid: Pid, target_pid: i32) -> Result<WaitResult, proc::WaitError> {
    // Get list of children from scheduler
    let children = sched::get_task_children(parent_pid);

    if children.is_empty() {
        return Err(proc::WaitError::NoChildren);
    }

    // Look for zombie children
    for child_pid in &children {
        // If target_pid > 0, only check that specific child
        if target_pid > 0 && *child_pid != target_pid as u32 {
            continue;
        }

        // Check if child is zombie
        if let Some(state) = sched::get_task_state(*child_pid) {
            if state == TaskState::TASK_ZOMBIE {
                // Found a zombie - get its exit status
                if let Some(status) = sched::get_task_exit_status(*child_pid) {
                    return Ok(WaitResult {
                        pid: *child_pid,
                        status,
                    });
                }
            }
        }
    }

    // No zombie found yet
    Err(proc::WaitError::WouldBlock)
}

/// Run a child process to completion
///
/// This function saves the parent's context and enters the child.
/// When the child exits, control returns to parent via sysretq.
#[allow(dead_code)]
pub fn run_child_process(child_pid: Pid) {
    let parent_pid = sched::current_pid().unwrap_or(0);

    // Get parent's PML4 for restoring later
    let _parent_pml4 = sched::get_task_meta(parent_pid)
        .map(|m| m.lock().address_space.pml4_phys().as_u64())
        .unwrap_or(unsafe { KERNEL_PML4 });

    // Get child process info from scheduler
    let (child_pml4, kernel_stack_phys, kernel_stack_size) = match sched::get_task_switch_info(child_pid) {
        Some((_, pml4, kstack, kstack_size)) => (pml4, kstack, kstack_size),
        None => return,
    };

    // Set current process to child
    sched::switch_to(child_pid);
    #[cfg(feature = "debug-fork")]
    {
        let verify_pid = sched::current_pid().unwrap_or(0);
        debug_fork!(
            "[RUN_CHILD] switch_to({}) done, verify={}",
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

    // Get child's saved context from scheduler
    let child_ctx = match sched::get_task_context(child_pid) {
        Some(ctx) => ProcessContext {
            rip: ctx.rip,
            rsp: ctx.rsp,
            rflags: ctx.rflags,
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
            cs: ctx.cs,
            ss: ctx.ss,
            fs_base: ctx.fs_base,
        },
        None => return,
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
        arch::enter_usermode_with_context(
            child_kernel_stack_top,
            child_pml4.as_u64(),
            &user_ctx,
            child_ctx.fs_base, // Pass FS base for TLS
        );
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
    let current_pid = sched::current_pid().unwrap_or(0);

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

    // Call do_exec - returns ExecResult with new address space and context
    match do_exec(
        &elf_data,
        &argv,
        &envp,
        mm(),
        kernel_pml4,
    ) {
        Ok(exec_result) => {
            // Get new address space PML4
            let new_pml4 = exec_result.address_space.pml4_phys();
            let ctx = &exec_result.context;

            // TEMP DEBUG: Print fs_base from exec_result
            {
                use core::fmt::Write;
                let mut writer = serial::SerialWriter;
                let _ = writeln!(writer, "[EXEC] do_exec returned fs_base={:#x}", ctx.fs_base);
            }

            // Build task context from exec result
            let task_ctx = TaskContext {
                rip: ctx.rip,
                rsp: ctx.rsp,
                rflags: ctx.rflags,
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
                cs: ctx.cs,
                ss: ctx.ss,
                fs_base: ctx.fs_base,
            };

            // Update ProcessMeta with new address space and cmdline
            if let Some(meta) = sched::get_task_meta(current_pid) {
                let mut m = meta.lock();
                m.address_space = exec_result.address_space;
                m.cmdline = exec_result.cmdline;
                m.environ = exec_result.environ;
            }

            // Update scheduler task with new exec info
            sched::update_task_exec_info(current_pid, new_pml4, ctx.rip, ctx.rsp, task_ctx);

            // Debug: print exec return values
            debug_fork!("[EXEC] Switching to PML4={:#x}", new_pml4.as_u64());
            debug_fork!("[EXEC] rip={:#x} rsp={:#x}", ctx.rip, ctx.rsp);
            debug_fork!("[EXEC] fs_base={:#x} (TLS)", ctx.fs_base);
            debug_fork!(
                "[EXEC] argc={} argv={:#x} envp={:#x}",
                ctx.rdi,
                ctx.rsi,
                ctx.rdx
            );

            // TEMP DEBUG: Print exec state before jumping
            {
                use core::fmt::Write;
                let mut writer = serial::SerialWriter;
                let _ = writeln!(writer, "[EXEC_DEBUG] About to jump to Python");
                let _ = writeln!(writer, "[EXEC_DEBUG] RIP={:#x} RSP={:#x}", ctx.rip, ctx.rsp);
                let _ = writeln!(writer, "[EXEC_DEBUG] FS_BASE={:#x}", ctx.fs_base);
                let _ = writeln!(writer, "[EXEC_DEBUG] argc={} argv={:#x} envp={:#x}",
                    ctx.rdi, ctx.rsi, ctx.rdx);
            }

            // Switch to new address space and jump to entry point
            unsafe {
                write_cr3(new_pml4);
                flush_tlb_all();

                // Return to user mode at new entry point
                // We use sysretq which expects: rcx = rip, r11 = rflags
                // Use explicit registers to prevent compiler from reusing registers
                // that we overwrite before their values are consumed
                core::arch::asm!(
                    // Set FS base MSR if fs_base is non-zero (TLS support)
                    "test r15, r15",
                    "jz 2f",
                    "mov ecx, 0xC0000100",  // MSR IA32_FS_BASE
                    "mov rax, r15",
                    "mov rdx, r15",
                    "shr rdx, 32",
                    "wrmsr",
                    "2:",
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
                    // Load user data segment selectors for DS/ES (0x1B = USER_DS | 3)
                    // NOTE: In x86-64 long mode, FS base comes from FS_BASE MSR, not segment descriptor.
                    // We do NOT load FS at all - just leave it as-is after WRMSR set the base.
                    // NOTE: Do NOT load GS - swapgs will handle it
                    "mov ax, 0x1b",
                    "mov ds, ax",
                    "mov es, ax",
                    // Do NOT touch FS - leave it alone after WRMSR
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
                    in("r15") ctx.fs_base,
                        options(noreturn)
                    );
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
