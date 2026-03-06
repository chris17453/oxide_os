//! Process management callbacks for the OXIDE kernel.
//!
//! Implements fork, exec, wait, and exit syscall handlers.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::ptr::addr_of_mut;
use core::sync::atomic::Ordering;

use arch_traits::Arch;
use crate::arch;
use mm_paging::{flush_tlb_all, phys_to_virt, write_cr3};
use os_core::PhysAddr;
use os_log::println;
use proc::{ProcessContext, ProcessMeta, WaitOptions, WaitResult, do_exec, do_fork};
use proc_traits::Pid;
use sched::TaskState;
use signal::{PendingSignals, SigSet};
use vfs::{VnodeType, mount::GLOBAL_VFS};

#[allow(unused_imports)]
use crate::debug_fork;
use crate::globals::{
    CHILD_DONE, KERNEL_PML4, PARENT_CONTEXT, ParentContext, USER_EXIT_STATUS, USER_EXITED,
};
use crate::scheduler::wake_parent;
use mm_manager::mm;
use sched::TaskContext;

fn trace_u64(mut n: u64) {
    unsafe {
        if n == 0 {
            os_log::write_byte_raw(b'0');
            return;
        }
        let mut buf = [0u8; 20];
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
}

fn trace_i32(n: i32) {
    unsafe {
        if n < 0 {
            os_log::write_byte_raw(b'-');
            trace_u64((-n) as u64);
        } else {
            trace_u64(n as u64);
        }
    }
}

/// — CrashBloom: Walk a PML4's user-half page tables and check for buddy allocator
/// FreeBlock canary corruption. If a PT structure frame was freed while still
/// referenced, the buddy writes 0x4652454542304C ("FREEBL0C") into its first
/// 8 bytes. We detect this by checking every intermediate entry's target frame.
///
/// Returns the number of corrupted entries found. Logs every hit to serial.
/// Call this after exec/fork to catch corruption before it causes triple faults.
unsafe fn validate_page_tables(pml4_phys: PhysAddr, label: &str) -> u32 {
    const FREE_BLOCK_MAGIC: u64 = 0x4652454542304C;
    let mut corrupt_count: u32 = 0;

    let pml4_virt = phys_to_virt(pml4_phys);
    let pml4 = unsafe { &*pml4_virt.as_ptr::<mm_paging::PageTable>() };

    // — CrashBloom: Check PML4 frame itself for corruption
    let pml4_first = unsafe { core::ptr::read_volatile(pml4_virt.as_ptr::<u64>()) };
    if pml4_first == FREE_BLOCK_MAGIC {
        unsafe {
            os_log::write_str_raw("[PT-CORRUPT] ");
            os_log::write_str_raw(label);
            os_log::write_str_raw(": PML4 frame IS a FreeBlock! pml4=0x");
            os_log::write_u64_hex_raw(pml4_phys.as_u64());
            os_log::write_str_raw("\n");
        }
        return 999; // catastrophic — PML4 itself is freed
    }

    for pml4_idx in 0..256usize {
        let pml4_entry = &pml4[pml4_idx];
        if !pml4_entry.is_present() {
            continue;
        }

        let pdpt_phys = pml4_entry.addr();
        let pdpt_virt = phys_to_virt(pdpt_phys);
        let pdpt_first = unsafe { core::ptr::read_volatile(pdpt_virt.as_ptr::<u64>()) };
        if pdpt_first == FREE_BLOCK_MAGIC {
            unsafe {
                os_log::write_str_raw("[PT-CORRUPT] ");
                os_log::write_str_raw(label);
                os_log::write_str_raw(": PDPT[");
                os_log::write_u32_raw(pml4_idx as u32);
                os_log::write_str_raw("] frame=0x");
                os_log::write_u64_hex_raw(pdpt_phys.as_u64());
                os_log::write_str_raw(" is FreeBlock!\n");
            }
            corrupt_count += 1;
            continue; // don't dereference further — the entries are garbage
        }

        let pdpt = unsafe { &*pdpt_virt.as_ptr::<mm_paging::PageTable>() };
        for pdpt_idx in 0..512usize {
            let pdpt_entry = &pdpt[pdpt_idx];
            if !pdpt_entry.is_present() || pdpt_entry.is_huge() {
                continue;
            }

            let pd_phys = pdpt_entry.addr();
            let pd_virt = phys_to_virt(pd_phys);
            let pd_first = unsafe { core::ptr::read_volatile(pd_virt.as_ptr::<u64>()) };
            if pd_first == FREE_BLOCK_MAGIC {
                unsafe {
                    os_log::write_str_raw("[PT-CORRUPT] ");
                    os_log::write_str_raw(label);
                    os_log::write_str_raw(": PD[");
                    os_log::write_u32_raw(pml4_idx as u32);
                    os_log::write_str_raw("][");
                    os_log::write_u32_raw(pdpt_idx as u32);
                    os_log::write_str_raw("] frame=0x");
                    os_log::write_u64_hex_raw(pd_phys.as_u64());
                    os_log::write_str_raw(" is FreeBlock!\n");
                }
                corrupt_count += 1;
                continue;
            }

            let pd = unsafe { &*pd_virt.as_ptr::<mm_paging::PageTable>() };
            for pd_idx in 0..512usize {
                let pd_entry = &pd[pd_idx];
                if !pd_entry.is_present() || pd_entry.is_huge() {
                    continue;
                }

                let pt_phys = pd_entry.addr();
                let pt_virt = phys_to_virt(pt_phys);
                let pt_first = unsafe { core::ptr::read_volatile(pt_virt.as_ptr::<u64>()) };
                if pt_first == FREE_BLOCK_MAGIC {
                    unsafe {
                        os_log::write_str_raw("[PT-CORRUPT] ");
                        os_log::write_str_raw(label);
                        os_log::write_str_raw(": PT[");
                        os_log::write_u32_raw(pml4_idx as u32);
                        os_log::write_str_raw("][");
                        os_log::write_u32_raw(pdpt_idx as u32);
                        os_log::write_str_raw("][");
                        os_log::write_u32_raw(pd_idx as u32);
                        os_log::write_str_raw("] frame=0x");
                        os_log::write_u64_hex_raw(pt_phys.as_u64());
                        os_log::write_str_raw(" is FreeBlock!\n");
                    }
                    corrupt_count += 1;
                    continue;
                }
            }
        }
    }

    corrupt_count
}

/// User exit function
///
/// ThreadRogue: Handle both thread and process exit - clean termination paths
pub fn user_exit(status: i32) -> ! {
    let current_tid = sched::current_pid().unwrap_or(0);

    // Get ProcessMeta to check if this is a thread or main process
    let is_thread = if let Some(meta_arc) = sched::get_task_meta(current_tid) {
        let meta = meta_arc.lock();
        let tgid = meta.tgid;
        let clear_child_tid = meta.clear_child_tid;
        let is_thread = current_tid != tgid;
        unsafe {
            os_log::write_str_raw("[EXIT] pid=");
            trace_u64(current_tid as u64);
            os_log::write_str_raw(" tgid=");
            trace_u64(tgid as u64);
            os_log::write_str_raw(" status=");
            trace_i32(status);
            os_log::write_str_raw(" thread=");
            os_log::write_byte_raw(if is_thread { b'1' } else { b'0' });
            os_log::write_str_raw("\n");
        }

        debug_proc!(
            "[EXIT] TID={} TGID={} status={} is_thread={}",
            current_tid,
            tgid,
            status,
            is_thread
        );

        // Handle thread exit - clear_child_tid and futex wake
        if is_thread && clear_child_tid != 0 {
            // — VeilAudit: Validate clear_child_tid is a userspace address before
            // writing. A malicious thread can set this to a kernel address and we'd
            // write zero to arbitrary kernel memory on exit. That's a trivial
            // privilege escalation. Check it's below the user/kernel boundary.
            const USER_SPACE_END: u64 = 0x0000_8000_0000_0000;
            if clear_child_tid < USER_SPACE_END {
                unsafe {
                    arch::user_access_begin();
                    let ptr = clear_child_tid as *mut i32;
                    core::ptr::write_volatile(ptr, 0);
                    arch::user_access_end();
                }
            }

            // Wake up any threads waiting on this futex
            if let Ok(pids) = proc::futex_wake(clear_child_tid, i32::MAX) {
                debug_proc!(
                    "[EXIT] Thread {} cleared tid at {:#x}, woke {} waiters",
                    current_tid,
                    clear_child_tid,
                    pids.len()
                );
                for pid in pids {
                    sched::wake_up(pid);
                }
            }
        }

        is_thread
    } else {
        false
    };

    if is_thread {
        // This is a thread exit (not the main process)
        // ThreadRogue: Thread cleanup - no zombie state, immediate removal
        debug_proc!(
            "[EXIT] Thread {} exiting (not becoming zombie)",
            current_tid
        );

        // Remove thread from scheduler immediately (no zombie for threads)
        crate::scheduler::remove_process(current_tid);

        // Reschedule to another task
        sched::set_need_resched();
        arch::allow_kernel_preempt();
        loop {
            arch::wait_for_interrupt();
        }
    } else {
        // This is the main process exit (thread group leader)
        // BlackLatch: Main process termination - zombie state for parent reaping
        let parent_pid = sched::get_task_ppid(current_tid).unwrap_or(0);
        unsafe {
            os_log::write_str_raw("[EXIT] zombie pid=");
            trace_u64(current_tid as u64);
            os_log::write_str_raw(" ppid=");
            trace_u64(parent_pid as u64);
            os_log::write_str_raw("\n");
        }

        debug_proc!(
            "[EXIT] Main process {} exiting, parent={}",
            current_tid,
            parent_pid
        );

        // — GraveShift: Close all file descriptors BEFORE zombie state (Linux exit_files).
        // The zombie only needs PID + exit status for waitpid. Keeping fds open is
        // catastrophic: a zombie holding PipeWrite keeps has_writers()=true → parent's
        // pipe read blocks forever → parent never reaches waitpid → deadlock.
        // Address space stays alive (zombie CR3 still loaded) — freed on waitpid reap.
        if let Some(meta_arc) = sched::get_task_meta(current_tid) {
            let mut meta = meta_arc.lock();
            meta.fd_table = vfs::FdTable::new();
            meta.shared_fd_table = None;
        }

        // — GraveShift: Encode exit status in Linux waitpid format.
        // Normal exit: bits [15:8] = exit code, bits [7:0] = 0.
        // waitpid consumers decode with WEXITSTATUS = (status >> 8) & 0xFF.
        sched::set_task_exit_status(current_tid, (status & 0xFF) << 8);

        unsafe {
            USER_EXIT_STATUS = status;
        }
        USER_EXITED.store(true, Ordering::SeqCst);

        // Wake parent if it's blocked waiting for us
        if parent_pid > 0 {
            unsafe {
                os_log::write_str_raw("[EXIT] waking parent pid=");
                trace_u64(parent_pid as u64);
                os_log::write_str_raw(" parent_state=");
                if let Some(st) = sched::get_task_state(parent_pid) {
                    trace_u64(st.0 as u64);
                } else {
                    os_log::write_str_raw("NONE");
                }
                os_log::write_str_raw("\n");
            }
            wake_parent(parent_pid);
            unsafe {
                os_log::write_str_raw("[EXIT] wake_parent done, parent_state=");
                if let Some(st) = sched::get_task_state(parent_pid) {
                    trace_u64(st.0 as u64);
                } else {
                    os_log::write_str_raw("NONE");
                }
                os_log::write_str_raw("\n");
            }
        }

        // Clear any stale PARENT_CONTEXT
        let _ = PARENT_CONTEXT.lock().take();

        // NOTE: Do NOT remove the process from scheduler here!
        // The process must stay as a zombie until the parent reaps it via wait().

        debug_proc!(
            "[EXIT] Process {} became zombie, woke parent={}",
            current_tid,
            parent_pid
        );
        sched::set_need_resched();
        arch::allow_kernel_preempt();
        loop {
            arch::wait_for_interrupt();
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
        gs_base: 0, // Will be set by arch_prctl(ARCH_SET_GS) if used
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

            // — BlackLatch: Clear the guard page PTE from the kernel direct map.
            // Stack overflow now causes an immediate #PF instead of silently
            // scribbling on whoever had the misfortune of living below us in RAM.
            // Do this BEFORE handing the stack address to any scheduler structures.
            if fork_result.guard_phys.as_u64() != 0 {
                unsafe {
                    crate::kstack_guard::unmap_guard_page(fork_result.guard_phys);
                }
            }

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
                gs_base: parent_context.gs_base,
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
                gs_base: child_ctx.gs_base, // fork inherits parent's GS base
            };

            // — BlackLatch: Create Task and stamp its context BEFORE enqueuing.
            // The old code called add_task() then set_task_context() — a microsecond
            // race where any CPU's timer tick could pick up the task with cs=0, ss=0,
            // rip=0 and iretq into a GPF. Now the context is baked in before the
            // scheduler ever sees it.
            let mut child_task = sched::Task::new_with_meta(
                child_pid,
                parent_pid,
                fork_result.kernel_stack_phys,
                fork_result.kernel_stack_size,
                child_pml4,
                child_ctx.rip,
                child_ctx.rsp,
                child_meta_arc,
            );
            child_task.context = child_task_ctx;

            // Add child to THIS CPU's scheduler — fork immediately switches on
            // the local CPU, so enqueueing remotely (via last_cpu default=0)
            // can leave rq.curr pointing to a PID with no local Task slot.
            let cpu = sched::this_cpu();
            sched::add_task_to_cpu(child_task, cpu);

            // Add child to parent's children list
            sched::add_task_child(parent_pid, child_pid);

            // Tell scheduler we're switching to child
            sched::switch_to(child_pid);
            // Force a scheduler pass on the next timer tick so the parent can
            // resume even if CFS's vruntime preemption heuristic doesn't fire.
            // Fork semantics need both tasks to make progress promptly.
            sched::set_need_resched();

            // Get child's kernel stack top
            let child_kstack_virt = phys_to_virt(fork_result.kernel_stack_phys);
            let child_kstack_top =
                child_kstack_virt.as_u64() + fork_result.kernel_stack_size as u64;

            // Update kernel stack
            // — CrashBloom: Use checked version in fork path (not hot path).
            // If GS_BASE got corrupted, we recover before the crash.
            arch::syscall::LAST_SET_KSTACK_SITE.store(2, core::sync::atomic::Ordering::Relaxed);
            unsafe {
                arch::syscall::set_kernel_stack_checked(child_kstack_top);
            }
            arch::gdt::set_kernel_stack(child_kstack_top);

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

            // — GraveShift: Capture for diagnostics before child_ctx is moved.
            let diag_rip = child_ctx.rip;
            let diag_rsp = child_ctx.rsp;
            // Per-call child context (stack local): avoids cross-CPU races from a
            // function-static context buffer when two CPUs fork concurrently.
            let mut fork_child_ctx = child_ctx;

            unsafe {
                // — GraveShift: Pre-switch diagnostic — validate child PML4 before loading CR3.
                {
                    let pml4_virt = mm_paging::phys_to_virt(child_pml4);
                    let entry_256 = core::ptr::read_volatile(pml4_virt.as_ptr::<u64>().add(256));
                    let entry_0 = core::ptr::read_volatile(pml4_virt.as_ptr::<u64>());
                    os_log::write_str_raw("[FORK-CR3] About to load child cr3=0x");
                    os_log::write_u64_hex_raw(child_pml4.as_u64());
                    os_log::write_str_raw(" PML4[0]=0x");
                    os_log::write_u64_hex_raw(entry_0);
                    os_log::write_str_raw(" PML4[256]=0x");
                    os_log::write_u64_hex_raw(entry_256);
                    os_log::write_str_raw("\n");
                    os_log::write_str_raw("[FORK-CR3] child RIP=0x");
                    os_log::write_u64_hex_raw(diag_rip);
                    os_log::write_str_raw(" child RSP=0x");
                    os_log::write_u64_hex_raw(diag_rsp);
                    os_log::write_str_raw("\n");
                }

                // Switch page tables
                arch::switch_page_table(os_core::PhysAddr::new(child_pml4.as_u64()));

                os_log::write_str_raw("[FORK-CR3] CR3 loaded OK, about to sysretq\n");

                let ctx_ptr = (&mut fork_child_ctx as *mut ProcessContext) as u64;

                // Child's fork() returns 0
                // TODO: move to arch crate — arch-specific context switch/usermode entry
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

/// Clone callback for syscalls - creates threads with CLONE_VM
///
/// Creates a child thread sharing address space with parent.
/// Returns child TID to parent, 0 to child.
///
/// GraveShift: Threading infrastructure - shared address space, separate execution contexts
pub fn kernel_clone(flags: u32, stack: u64, parent_tid: u64, child_tid: u64, tls: u64) -> i64 {
    use alloc::sync::Arc;
    use proc::{CloneArgs, do_clone};
    use spin::Mutex;

    let parent_pid = sched::current_pid().unwrap_or(0);
    debug_proc!(
        "[CLONE] Clone called from PID {} with flags={:#x}",
        parent_pid,
        flags
    );

    // Get parent's ProcessMeta
    let parent_meta_arc = match sched::get_task_meta(parent_pid) {
        Some(m) => m,
        None => {
            debug_proc!("[CLONE] Parent meta not found");
            return -3; // ESRCH
        }
    };

    // Get current process context from syscall
    let user_ctx = arch::get_user_context();

    // Get parent's fs_base and gs_base from task context (for TLS)
    let (parent_fs_base, parent_gs_base) = if let Some(ctx) = sched::get_task_context(parent_pid) {
        (ctx.fs_base, ctx.gs_base)
    } else {
        (0, 0)
    };

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
        fs_base: parent_fs_base,
        gs_base: parent_gs_base,
    };

    const KERNEL_STACK_SIZE: usize = 128 * 1024;

    // Prepare clone arguments
    let args = CloneArgs {
        flags,
        stack,
        parent_tid,
        child_tid,
        tls,
    };

    // Call do_clone
    let parent_meta = parent_meta_arc.lock();
    // — ColdCipher: Inherit the ASLR-seeded mmap hint from the parent so the
    // new thread starts allocating from the same randomized region. Without this
    // the thread's first mmap(NULL) falls back to the hardcoded default and the
    // parent's ASLR jitter is wasted.
    let parent_mmap_hint = parent_meta.next_mmap_addr;
    let result = do_clone(
        parent_pid,
        &parent_meta,
        &parent_context,
        &args,
        mm(),
        KERNEL_STACK_SIZE,
    );
    drop(parent_meta); // Release lock

    match result {
        Ok(clone_result) => {
            let child_tid = clone_result.child_tid;
            debug_proc!(
                "[CLONE] Created thread TID {} in TGID {}",
                child_tid,
                clone_result.tgid
            );

            // — BlackLatch: Unmap the guard page below this thread's kernel stack.
            // Same deal as fork: overflow #PFs instead of silently eating RAM.
            if clone_result.guard_phys.as_u64() != 0 {
                unsafe {
                    crate::kstack_guard::unmap_guard_page(clone_result.guard_phys);
                }
            }

            // — VeilAudit: Write child TID to parent's memory, but ONLY if the
            // address is in userspace. A crafted clone3 call could pass a kernel
            // address here and get an arbitrary write primitive. Validate first.
            if clone_result.parent_tid_addr != 0
                && clone_result.parent_tid_addr < 0x0000_8000_0000_0000
            {
                unsafe {
                    arch::user_access_begin();
                    let ptr = clone_result.parent_tid_addr as *mut i32;
                    core::ptr::write_volatile(ptr, child_tid as i32);
                    arch::user_access_end();
                }
            }

            // Create child's ProcessMeta (shared for threads)
            let mut child_meta = ProcessMeta {
                tgid: clone_result.tgid,
                pgid: clone_result.pgid,
                sid: clone_result.sid,
                credentials: clone_result.credentials,
                address_space: unsafe {
                    proc::UserAddressSpace::from_raw(
                        clone_result.shared_address_space.lock().pml4_phys(),
                        alloc::vec![],
                        mm_vma::VmAreaList::new(),
                    )
                },
                shared_address_space: Some(clone_result.shared_address_space.clone()),
                fd_table: if let Some(ref shared) = clone_result.shared_fd_table {
                    shared.lock().clone_for_fork()
                } else {
                    vfs::FdTable::new()
                },
                shared_fd_table: clone_result.shared_fd_table,
                signal_mask: SigSet::empty(),
                pending_signals: PendingSignals::new(),
                sigactions: clone_result.sigactions,
                cwd: clone_result.cwd.clone(),
                cmdline: alloc::vec![],
                environ: alloc::vec![],
                tls: clone_result.tls,
                clear_child_tid: clone_result.clear_child_tid,
                owned_frames: alloc::vec![],
                guard_pages: alloc::vec![], // Guard pages assigned after clone in kernel crate
                alarm_remaining: 0,
                itimer_real_interval_us: 0,
                itimer_real_value_us: 0,
                itimer_virtual_interval_us: 0,
                itimer_virtual_value_us: 0,
                itimer_prof_interval_us: 0,
                itimer_prof_value_us: 0,
                is_thread_leader: false, // This is a thread, not the leader
                thread_group: alloc::vec![],
                umask: 0o022,
                program_break: 0,
                // — ColdCipher: Inherit the parent's ASLR-randomized mmap base.
                // Threads share the same address space — using the same hint prevents
                // two threads from independently allocating at the same virtual address.
                next_mmap_addr: parent_mmap_hint,
                cpu_time_ns: 0,
                stop_signal: None,
                continued: false,
                tty_nr: 0, // Inherit controlling terminal (0 for now)
            };

            // — BlackLatch: Thread kernel stack cleanup — register the full
            // allocation (guard + stack pages) in owned_frames, and the guard
            // address in guard_pages. Drop will remap the guard before freeing.
            // alloc_base = guard_phys = one page below kernel_stack_phys.
            {
                let kernel_stack_pages = KERNEL_STACK_SIZE / 4096;
                let total_pages = kernel_stack_pages + 1;
                child_meta.add_owned_frames(clone_result.guard_phys, total_pages);
                if clone_result.guard_phys.as_u64() != 0 {
                    child_meta.add_guard_page(clone_result.guard_phys);
                }
            }

            let child_meta_arc = Arc::new(Mutex::new(child_meta));

            // Get child's PML4 for page table
            let child_pml4 = clone_result.shared_address_space.lock().pml4_phys();

            // Create parent's TaskContext (returns child TID)
            let parent_task_ctx = TaskContext {
                rip: parent_context.rip,
                rsp: parent_context.rsp,
                rflags: parent_context.rflags,
                rax: child_tid as u64, // Parent returns child TID
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
                cs: 0x23,
                ss: 0x1B,
                fs_base: parent_context.fs_base,
                gs_base: parent_context.gs_base,
            };
            sched::set_task_context(parent_pid, parent_task_ctx);

            // Create child's TaskContext (returns 0)
            let child_task_ctx = TaskContext {
                rip: clone_result.child_context.rip,
                rsp: clone_result.child_context.rsp,
                rflags: clone_result.child_context.rflags,
                rax: 0, // Child returns 0
                rbx: clone_result.child_context.rbx,
                rcx: clone_result.child_context.rcx,
                rdx: clone_result.child_context.rdx,
                rsi: clone_result.child_context.rsi,
                rdi: clone_result.child_context.rdi,
                rbp: clone_result.child_context.rbp,
                r8: clone_result.child_context.r8,
                r9: clone_result.child_context.r9,
                r10: clone_result.child_context.r10,
                r11: clone_result.child_context.r11,
                r12: clone_result.child_context.r12,
                r13: clone_result.child_context.r13,
                r14: clone_result.child_context.r14,
                r15: clone_result.child_context.r15,
                cs: 0x23,
                ss: 0x1B,
                fs_base: clone_result.tls, // Set TLS for child
                gs_base: parent_context.gs_base, // clone inherits parent's GS base
            };

            // — VeilAudit: Same validation for child_tid_addr. The child's address
            // space is the parent's (CLONE_VM), so a kernel pointer here would
            // scribble on kernel memory from the child's perspective too.
            if clone_result.child_tid_addr != 0
                && clone_result.child_tid_addr < 0x0000_8000_0000_0000
            {
                unsafe {
                    arch::user_access_begin();
                    let ptr = clone_result.child_tid_addr as *mut i32;
                    core::ptr::write_volatile(ptr, child_tid as i32);
                    arch::user_access_end();
                }
            }

            // — BlackLatch: Same race fix as fork — stamp context before enqueue.
            // clone() was just as vulnerable: add_task() with zeroed context, then
            // set_task_context() a few instructions later. At 400Hz across 4 CPUs,
            // that microsecond window was a ticking bomb.
            let mut child_task = sched::Task::new_with_meta(
                child_tid,
                parent_pid,
                clone_result.kernel_stack_phys,
                clone_result.kernel_stack_size,
                child_pml4,
                clone_result.child_context.rip,
                clone_result.child_context.rsp,
                child_meta_arc,
            );
            child_task.context = child_task_ctx;

            // Keep clone child local for the same reason as fork: the caller's
            // CPU is about to run/schedule it, so avoid remote enqueue by default.
            let cpu = sched::this_cpu();
            sched::add_task_to_cpu(child_task, cpu);

            // ThreadRogue: New execution context spawned, ready for the scheduler
            debug_proc!("[CLONE] Thread {} created successfully", child_tid);
            child_tid as i64
        }
        Err(e) => {
            debug_proc!("[CLONE] Clone failed: {:?}", e);
            -22 // EINVAL
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
    #[cfg(feature = "debug-proc")]
    unsafe {
        os_log::write_str_raw("[WAIT] enter ppid=");
        trace_u64(parent_pid as u64);
        os_log::write_str_raw(" target=");
        trace_i32(pid);
        os_log::write_str_raw(" opts=");
        trace_i32(options);
        os_log::write_str_raw("\n");
    }

    debug_proc!("[WAIT] pid={} waiting for child={}", parent_pid, pid);
    loop {
        // — GraveShift: Trace each waitpid iteration — gated behind debug-proc
        #[cfg(feature = "debug-proc")]
        unsafe {
            os_log::write_str_raw("[WAIT-LOOP] ppid=");
            trace_u64(parent_pid as u64);
            os_log::write_str_raw(" target=");
            trace_i32(pid);
            os_log::write_str_raw("\n");
        }
        // Check for state changes: zombie, stopped (WUNTRACED), continued (WCONTINUED)
        match find_child_state_change(parent_pid, pid, &wait_opts) {
            Ok(result) => {
                #[cfg(feature = "debug-proc")]
                unsafe {
                    os_log::write_str_raw("[WAIT] hit ppid=");
                    trace_u64(parent_pid as u64);
                    os_log::write_str_raw(" child=");
                    trace_u64(result.pid as u64);
                    os_log::write_str_raw(" status=");
                    trace_i32(result.status);
                    os_log::write_str_raw("\n");
                }
                debug_proc!(
                    "[WAIT] pid={} found state change: child={} status=0x{:x}",
                    parent_pid,
                    result.pid,
                    result.status
                );

                // Only reap zombies (low 7 bits != 0x7F means not stopped)
                let is_stopped = (result.status & 0xFF) == 0x7F;
                let is_continued = result.status == 0xFFFF;
                if !is_stopped && !is_continued {
                    crate::scheduler::remove_process(result.pid);
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
                        #[cfg(feature = "debug-proc")]
                        unsafe {
                            os_log::write_str_raw("[WAIT] block ppid=");
                            trace_u64(parent_pid as u64);
                            os_log::write_str_raw(" target=");
                            trace_i32(pid);
                            os_log::write_str_raw("\n");
                        }

                        // Update scheduler's Task state bookkeeping.
                        sched::set_task_waiting(parent_pid, pid);
                        // Request reschedule and cooperatively sleep one interrupt.
                        //
                        // NOTE: Do NOT hard-block here via block_current(). There is
                        // a lost-wakeup window:
                        //   1) find_child_state_change() says WouldBlock
                        //   2) child exits and wake_parent() runs
                        //   3) parent executes block_current() and sleeps forever
                        // because the wake already happened. Using sti+hlt polling
                        // avoids that deadlock while still yielding CPU.
                        sched::set_need_resched();

                        // Allow scheduler to preempt us while we wait
                        arch::allow_kernel_preempt();

                        // Wait for timer interrupt - scheduler will run other processes
                        // When child exits, wake_parent() will wake us up
                        // NOTE: sti + hlt must be in the same asm block to avoid
                        // an extra tick delay if a timer fires between them.
                        arch::wait_for_interrupt();

                        // Clear preempt flag if we're still running
                        arch::disallow_kernel_preempt();

                        // — GraveShift: Check for pending signals after wakeup. If the
                        // current process has an actionable signal (not SIG_IGN, not blocked),
                        // bail with EINTR. check_signals_on_syscall_return() delivers on exit.
                        // Without this, waitpid looped forever even when the parent had pending
                        // signals — it only checked for child state changes, never its own signals.
                        if let Some(meta_arc) = sched::get_task_meta(parent_pid) {
                            if let Some(meta) = meta_arc.try_lock() {
                                if signal::delivery::should_interrupt_for_signal(
                                    &meta.pending_signals.set(),
                                    &meta.signal_mask,
                                    &meta.sigactions,
                                ) {
                                    return -4; // EINTR
                                }
                            }
                        }

                        continue;
                    }
                }
            }
        }
    }
}

/// Find a child with a reportable state change (zombie, stopped, continued)
///
/// — ThreadRogue: scanning the child roster for state transitions
fn find_child_state_change(
    parent_pid: Pid,
    target_pid: i32,
    opts: &WaitOptions,
) -> Result<WaitResult, proc::WaitError> {
    let children = sched::get_task_children(parent_pid);
    #[cfg(feature = "debug-proc")]
    unsafe {
        os_log::write_str_raw("[WAIT] scan ppid=");
        trace_u64(parent_pid as u64);
        os_log::write_str_raw(" target=");
        trace_i32(target_pid);
        os_log::write_str_raw(" nchild=");
        trace_u64(children.len() as u64);
        os_log::write_str_raw("\n");
    }

    if children.is_empty() {
        return Err(proc::WaitError::NoChildren);
    }

    for child_pid in &children {
        if target_pid > 0 && *child_pid != target_pid as u32 {
            continue;
        }

        if let Some(state) = sched::get_task_state(*child_pid) {
            // Zombie — always reportable
            if state == TaskState::TASK_ZOMBIE {
                if let Some(status) = sched::get_task_exit_status(*child_pid) {
                    return Ok(WaitResult {
                        pid: *child_pid,
                        status,
                    });
                }
            }

            // Stopped child — report if WUNTRACED requested
            if opts.untraced && state == TaskState::TASK_STOPPED {
                if let Some(meta_arc) = sched::get_task_meta(*child_pid) {
                    if let Some(mut meta) = meta_arc.try_lock() {
                        if let Some(sig) = meta.stop_signal.take() {
                            return Ok(WaitResult::stopped(*child_pid, sig as i32));
                        }
                    }
                }
            }

            // Continued child — report if WCONTINUED requested
            if opts.continued {
                if let Some(meta_arc) = sched::get_task_meta(*child_pid) {
                    if let Some(mut meta) = meta_arc.try_lock() {
                        if meta.continued {
                            meta.continued = false;
                            return Ok(WaitResult::continued(*child_pid));
                        }
                    }
                }
            }
        }
    }

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
    let (child_pml4, kernel_stack_phys, kernel_stack_size) =
        match sched::get_task_switch_info(child_pid) {
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
    // — CrashBloom: Use checked version in run_child_process (not hot path).
    arch::syscall::LAST_SET_KSTACK_SITE.store(3, core::sync::atomic::Ordering::Relaxed);
    unsafe {
        arch::syscall::set_kernel_stack_checked(child_kernel_stack_top);
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
            gs_base: ctx.gs_base,
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
            let current_cr3 = arch::read_page_table_root().as_u64();
            debug_fork!("[CHILD] Current CR3: {:#x}", current_cr3);
            debug_fork!("[CHILD] Child PML4: {:#x}", child_pml4.as_u64());

            // Switch to child's page tables
            arch::switch_page_table(os_core::PhysAddr::new(child_pml4.as_u64()));

            // Read back from the copied context
            let read_rax = *dest_ptr.add(0);
            let read_rcx = *dest_ptr.add(2);
            let read_rip = *dest_ptr.add(16);

            // Switch back to original page tables
            arch::switch_page_table(os_core::PhysAddr::new(current_cr3));

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

    // — GraveShift: unconditional exec trace for boot debugging
    unsafe { os_log::write_str_raw("[EXEC] "); }
    unsafe { os_log::write_str_raw(path); }
    unsafe { os_log::write_str_raw("\n"); }

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
                    Ok(s) => {
                        debug_fork!("[EXEC] argv[{}] = \"{}\" (len={})", i, s, len);
                        argv.push(String::from(s));
                    }
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

    // — GraveShift: VFS lookup no longer needs manual kpo toggling. KernelMutex on
    // the heap allocator handles preemption automatically — when heap locks are held
    // (preempt_count > 0), scheduler backs off. When in virtio-blk polling (no locks
    // held, count == 0), scheduler can preempt freely. The old manual kpo around this
    // block caused Build 67's heap deadlocks: kpo=true → preempted while holding heap
    // lock → next task deadlocks forever. Linux-model preempt_count kills that pattern.

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

    // — GraveShift: Smart ELF loading — read ONLY the bytes we need, not the whole file.
    // Debug info, symbol tables, and section headers can bloat an ELF to 20MB+ but we
    // only need the ELF header, program headers, and LOAD/TLS segment data. A 500MB
    // binary with 4MB of segments? We read 4MB. No artificial limits.
    let file_size = vnode.size() as usize;
    debug_fork!("[EXEC] File size: {} bytes", file_size);

    if file_size < 64 {
        debug_fork!("[EXEC] File too small for ELF header");
        return -8; // ENOEXEC
    }

    // Step 1: Read the ELF header (64 bytes) to find program header table
    let mut header_buf = [0u8; 64];
    match vnode.read(0, &mut header_buf) {
        Ok(n) if n >= 64 => {}
        _ => {
            debug_fork!("[EXEC] Failed to read ELF header");
            return -5; // EIO
        }
    }

    // Validate ELF magic
    if header_buf[0..4] != [0x7f, b'E', b'L', b'F'] {
        debug_fork!("[EXEC] Not an ELF file");
        return -8; // ENOEXEC
    }

    // Parse header fields we need
    let e_phoff = u64::from_le_bytes(header_buf[32..40].try_into().unwrap()) as usize;
    let e_phentsize = u16::from_le_bytes(header_buf[54..56].try_into().unwrap()) as usize;
    let e_phnum = u16::from_le_bytes(header_buf[56..58].try_into().unwrap()) as usize;

    // Step 2: Read program headers to find max file extent needed
    let ph_table_size = e_phentsize * e_phnum;
    let ph_table_end = e_phoff + ph_table_size;

    // Allocate buffer for program headers
    let mut ph_buf = alloc::vec![0u8; ph_table_size];
    match vnode.read(e_phoff as u64, &mut ph_buf) {
        Ok(n) if n >= ph_table_size => {}
        _ => {
            debug_fork!("[EXEC] Failed to read program headers");
            return -5; // EIO
        }
    }

    // Scan all program headers to find the maximum file offset we need to read
    let mut max_file_extent: usize = ph_table_end; // at minimum, need the headers
    for i in 0..e_phnum {
        let ph_start = i * e_phentsize;
        if ph_start + 56 > ph_buf.len() {
            break;
        }
        let p_type = u32::from_le_bytes(ph_buf[ph_start..ph_start + 4].try_into().unwrap());
        let p_offset =
            u64::from_le_bytes(ph_buf[ph_start + 8..ph_start + 16].try_into().unwrap()) as usize;
        let p_filesz =
            u64::from_le_bytes(ph_buf[ph_start + 32..ph_start + 40].try_into().unwrap()) as usize;

        // PT_LOAD=1, PT_TLS=7 — these are the only segment types we need file data for
        if (p_type == 1 || p_type == 7) && p_filesz > 0 {
            let extent = p_offset + p_filesz;
            if extent > max_file_extent {
                max_file_extent = extent;
            }
        }
    }

    // Cap to actual file size
    let read_size = max_file_extent.min(file_size);
    debug_fork!(
        "[EXEC] Reading {} of {} bytes (segments end at {})",
        read_size,
        file_size,
        max_file_extent
    );

    unsafe {
        os_log::write_str_raw("[EXEC] reading ");
        os_log::write_u64_hex_raw(read_size as u64);
        os_log::write_str_raw(" of ");
        os_log::write_u64_hex_raw(file_size as u64);
        os_log::write_str_raw(" bytes\n");
    }

    let mut elf_data = alloc::vec![0u8; read_size];

    let read_result = vnode.read(0, &mut elf_data);
    let bytes_read = match read_result {
        Ok(n) => n,
        Err(_e) => {
            debug_fork!("[EXEC] Read failed: {:?}", _e);
            return -5; // EIO
        }
    };

    if bytes_read < read_size {
        debug_fork!("[EXEC] Short read: {} of {} bytes", bytes_read, read_size);
        return -5; // EIO
    }
    debug_fork!("[EXEC] Read {} bytes, calling do_exec", bytes_read);

    // — GraveShift: trace read completion
    unsafe { os_log::write_str_raw("[EXEC] read done, calling do_exec\n"); }

    // Get kernel PML4 for creating new address space
    let kernel_pml4 = PhysAddr::new(unsafe { KERNEL_PML4 });

    // Call do_exec - returns ExecResult with new address space and context
    match do_exec(&elf_data, &argv, &envp, mm(), kernel_pml4) {
        Ok(exec_result) => {
            unsafe { os_log::write_str_raw("[EXEC] do_exec OK\n"); }
            // Get new address space PML4
            let new_pml4 = exec_result.address_space.pml4_phys();

            // — CrashBloom: Validate new address space PT integrity immediately
            // after do_exec. If corruption exists HERE, the bug is in do_exec or
            // the buddy allocator (double-alloc). If clean here but corrupt later,
            // it's a race between Drop and CR3.
            #[cfg(debug_assertions)]
            {
                let n = unsafe { validate_page_tables(new_pml4, "post-do_exec") };
                if n > 0 {
                    unsafe {
                        os_log::write_str_raw("[EXEC] ABORT: new PT corrupt after do_exec (");
                        os_log::write_u32_raw(n);
                        os_log::write_str_raw(" entries)\n");
                    }
                    return -5; // ENOMEM-ish — refuse to enter corrupted address space
                }
            }

            // — BlackLatch: Extract ALL values from exec_result.context before moving
            // address_space. We need local copies because Rust won't let us reference
            // exec_result after partial move, and we need these for both task_ctx and
            // user_ctx. Copy once, use everywhere.
            let entry_rip = exec_result.context.rip;
            let entry_rsp = exec_result.context.rsp;
            let entry_rflags = exec_result.context.rflags;
            let entry_rax = exec_result.context.rax;
            let entry_rbx = exec_result.context.rbx;
            let entry_rcx = exec_result.context.rcx;
            let entry_rdx = exec_result.context.rdx;
            let entry_rsi = exec_result.context.rsi;
            let entry_rdi = exec_result.context.rdi;
            let entry_rbp = exec_result.context.rbp;
            let entry_r8 = exec_result.context.r8;
            let entry_r9 = exec_result.context.r9;
            let entry_r10 = exec_result.context.r10;
            let entry_r11 = exec_result.context.r11;
            let entry_r12 = exec_result.context.r12;
            let entry_r13 = exec_result.context.r13;
            let entry_r14 = exec_result.context.r14;
            let entry_r15 = exec_result.context.r15;
            let entry_cs = exec_result.context.cs;
            let entry_ss = exec_result.context.ss;
            let entry_fs_base = exec_result.context.fs_base;
            let exec_mmap_base = exec_result.mmap_base;

            // Build task context from exec result
            // — SableWire: gs_base resets to 0 on exec — the old handler address
            // points into a dead address space. User can call arch_prctl(ARCH_SET_GS)
            // again after exec if they need GS-based TLS.
            let task_ctx = TaskContext {
                rip: entry_rip,
                rsp: entry_rsp,
                rflags: entry_rflags,
                rax: entry_rax,
                rbx: entry_rbx,
                rcx: entry_rcx,
                rdx: entry_rdx,
                rsi: entry_rsi,
                rdi: entry_rdi,
                rbp: entry_rbp,
                r8: entry_r8,
                r9: entry_r9,
                r10: entry_r10,
                r11: entry_r11,
                r12: entry_r12,
                r13: entry_r13,
                r14: entry_r14,
                r15: entry_r15,
                cs: entry_cs,
                ss: entry_ss,
                fs_base: entry_fs_base,
                gs_base: 0, // exec resets GS base — new process sets its own
            };

            // Update ProcessMeta with new address space and cmdline
            // — GraveShift: If get_task_meta returns None, the current PID doesn't
            // exist in the scheduler. That should be impossible — we're literally
            // running as this PID right now. But if it ever happens, continuing would
            // leak the entire new address space: exec_result.address_space never gets
            // stored, enter_usermode never returns, Drop never runs. The old address
            // space (wherever it is) also gets abandoned. Total frame hemorrhage.
            // Fail the exec instead — the process keeps its old image and gets -ESRCH.
            let meta = match sched::get_task_meta(current_pid) {
                Some(m) => m,
                None => {
                    unsafe { os_log::write_str_raw("[EXEC] FATAL: get_task_meta=None for running PID\n"); }
                    // — GraveShift: exec_result.address_space is dropped here, which
                    // triggers UserAddressSpace::Drop — all frames get freed properly.
                    return -3; // ESRCH
                }
            };

            // — BlackLatch: CRITICAL ORDERING — update task.pml4_phys BEFORE dropping
            // the old address space. The syscall handler runs with interrupts enabled
            // (STI at syscall entry line 266). A timer interrupt between "old PML4 freed"
            // and "task.pml4_phys updated" lets the scheduler context-switch this task
            // with the stale (freed) PML4. When we're switched back in, CR3 loads the
            // freed frame — PML4[0] = FREE_BLOCK_MAGIC (buddy canary) — and the first
            // user page access faults with "PML4 entry not present". Updating the task
            // FIRST ensures every possible context switch uses either the old (still
            // allocated) or new (properly initialized) PML4. — BlackLatch
            // — GraveShift: CRITICAL — disable preemption for the entire
            // update_task_exec_info → CR3 write → meta.address_space swap sequence.
            // A timer ISR context switch in the middle leaves us with a half-updated
            // state: scheduler sees new PML4 but CR3 still points to old, or vice
            // versa. Either direction = use-after-free on the old PML4 frame.
            // One atomic transaction, no windows. — GraveShift
            arch::preempt_disable();

            sched::update_task_exec_info(current_pid, new_pml4, entry_rip, entry_rsp, task_ctx);

            // — GraveShift: CRITICAL FIX — Switch CR3 to new PML4 BEFORE dropping
            // the old address space. update_task_exec_info only stores the new PML4
            // in the task struct for future context switches. It does NOT change the
            // hardware CR3 register. So right now, CR3 still points to the OLD PML4.
            //
            // When the Drop below frees the old PML4 frame, the buddy allocator
            // writes FreeBlock{magic, next, prev} to its first 24 bytes. If the freed
            // frame is then reallocated and zeroed (e.g., for another process's data
            // page or page table), PML4[256-511] — the kernel higher-half mappings —
            // become zeros. Any kernel TLB miss after that point reads "not present"
            // for kernel addresses → page fault in ring 0 → double fault → triple
            // fault → QEMU reset. Intermittent because it depends on how fast the
            // freed frame is recycled.
            //
            // By switching CR3 here, the hardware uses the new (valid) PML4 for all
            // page table walks. The old PML4 can be safely freed because no hardware
            // register references it. The new PML4 has identical PML4[256-511] kernel
            // mappings, so kernel code continues seamlessly. — GraveShift
            unsafe {
                arch::switch_page_table(os_core::PhysAddr::new(new_pml4.as_u64()));
            }

            // NOW safe to replace address_space — CR3 points to new PML4 and
            // task.pml4_phys also points to new PML4. No hardware or scheduler
            // reference to the old PML4 remains. The old address space Drop frees
            // the old PML4 and all its PT structure frames harmlessly.
            {
                let mut m = meta.lock();
                m.address_space = exec_result.address_space;
                m.cmdline = exec_result.cmdline;
                m.environ = exec_result.environ;

                // — ColdCipher: Install the ASLR-randomized mmap base from do_exec.
                // Each exec gets a fresh random starting point for anonymous mappings
                // so shared libraries and heap never land at the same address twice.
                // Without this update, mmap(NULL) would reuse the old (or default)
                // base — which makes ASLR for the stack pointless because ld.so is
                // still predictable.
                m.next_mmap_addr = exec_mmap_base;

                // — NeonRoot: Register the heap VMA so brk/sbrk can find and extend it.
                // Initial heap spans [0x600000, program_break) — may be zero-length if
                // the binary hasn't called brk yet. That's fine; sys_brk will extend it.
                let pb = m.program_break;
                let _ = m.address_space.add_vma(mm_vma::VmArea::new_named(
                    0x600000,
                    pb,
                    mm_vma::VmFlags::READ | mm_vma::VmFlags::WRITE,
                    mm_vma::VmType::Heap,
                    b"[heap]",
                ));

                // — GraveShift: POSIX exec signal reset. Caught handlers point into the OLD
                // address space — calling them after exec = instant GPF. SIG_IGN survives exec
                // (POSIX says so), SIG_DFL is already SIG_DFL. Everything else → SIG_DFL.
                // Linux does this in flush_signal_handlers(). We do it here because we're not
                // Linux and we don't have that many layers of indirection. — GraveShift
                for i in 0..signal::NSIG {
                    let action = &m.sigactions[i];
                    if action.handler().is_user_handler() {
                        m.sigactions[i] = signal::SigAction::new();
                    }
                }

                // — GraveShift: Clear pending signals on exec. The old process image is gone;
                // signals queued for it are meaningless now. Fresh start. — GraveShift
                m.pending_signals = signal::PendingSignals::new();
            }

            // — GraveShift: Transaction complete. Scheduler metadata, CR3, and
            // ProcessMeta.address_space all point to the new PML4. Safe to let
            // timer ISR preempt us again. — GraveShift
            arch::preempt_enable();

            // — CrashBloom: Post-Drop validation. The old address space is dead.
            // CR3 already points to new_pml4 (switched above). Verify the new PT
            // tree survived the Drop's frame freeing with no collateral damage.
            #[cfg(debug_assertions)]
            {
                let n = unsafe { validate_page_tables(new_pml4, "post-drop") };
                if n > 0 {
                    unsafe {
                        os_log::write_str_raw("[EXEC] WARNING: PT corrupt after Drop (");
                        os_log::write_u32_raw(n);
                        os_log::write_str_raw(" entries) — CR3 fix may not be sufficient\n");
                    }
                }
            }

            // Get current task's kernel stack for safe transition
            let kernel_stack_top = if let Some((kstack_phys, kstack_size)) =
                sched::get_task_kernel_stack(current_pid)
            {
                let kstack_virt = phys_to_virt(kstack_phys);
                kstack_virt.as_u64() + kstack_size as u64
            } else {
                // — GraveShift: Fallback — should never happen, but don't crash the world
                debug_fork!(
                    "[EXEC] WARNING - could not get task kernel stack for PID {}",
                    current_pid
                );
                0xffff_8000_0100_0000 // default kernel stack location
            };

            // Debug: print exec return values
            debug_fork!("[EXEC] Switching to PML4={:#x}", new_pml4.as_u64());
            debug_fork!("[EXEC] rip={:#x} rsp={:#x}", entry_rip, entry_rsp);
            debug_fork!("[EXEC] fs_base={:#x} (TLS)", entry_fs_base);

            // Create UserContext for enter_usermode_with_context
            // This function will copy the context to the kernel stack BEFORE switching CR3
            let user_ctx = arch::UserContext {
                rax: 0,
                rbx: entry_rbx,
                rcx: entry_rcx,
                rdx: entry_rdx,
                rsi: entry_rsi,
                rdi: entry_rdi,
                rbp: entry_rbp,
                rsp: entry_rsp,
                r8: entry_r8,
                r9: entry_r9,
                r10: entry_r10,
                r11: entry_r11,
                r12: entry_r12,
                r13: entry_r13,
                r14: entry_r14,
                r15: entry_r15,
                rip: entry_rip,
                rflags: 0x202, // IF set
            };

            // Use enter_usermode_with_context for safe transition
            // This copies the context to kernel stack BEFORE switching page tables
            unsafe {
                arch::enter_usermode_with_context(
                    kernel_stack_top,
                    new_pml4.as_u64(),
                    &user_ctx,
                    entry_fs_base,
                );
            }

            // Never reached
            0
        }
        Err(e) => {
            unsafe { os_log::write_str_raw("[EXEC] FAILED: "); }
            unsafe { os_log::write_str_raw(match e {
                proc::ExecError::InvalidElf => "InvalidElf",
                proc::ExecError::OutOfMemory => "OOM",
                proc::ExecError::ProcessNotFound => "ProcessNotFound",
                proc::ExecError::InvalidAddress => "InvalidAddress",
                proc::ExecError::InvalidArgument => "InvalidArgument",
            }); }
            unsafe { os_log::write_str_raw("\n"); }
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
