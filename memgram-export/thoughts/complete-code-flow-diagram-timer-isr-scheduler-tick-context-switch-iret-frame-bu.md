# Complete code flow diagram: timer ISR → scheduler_tick → context switch → iret frame building

📌 Pinned 🗄️ Archived

| Field | Value |
|-------|-------|
| ID | `0702a227c10c` |
| Type | observation |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-03T01:59:48.746502+00:00 |
| Accessed | 0 times |
| Keywords | timer-interrupt, scheduler-tick, context-switch-transaction, iret-frame, kernel-stack, user-rsp, frame-placement, critical-bug |

## Content

## COMPLETE CODE FLOW & REGISTER TRACE

### Phase 1: Timer Interrupt Delivery (kernel/arch/arch-x86_64/src/exceptions.rs:386-445)

```asm
timer_interrupt():
    test qword ptr [rsp + 8], 3    # Check CS & 3 (user mode?)
    jz 2f
    swapgs                         # If user mode, swap GS to kernel
2:
    push rax; push rbx; ... push r15  # Save all registers → InterruptFrame

    # At this point, stack layout (with example values):
    # rsp->[r15][r14]...[rax][rip][cs][rflags][user_rsp][ss]
    #      ↑ start of InterruptFrame
    
    mov rdi, rsp                   # Pass current_rsp = &InterruptFrame
    call handle_timer              # handle_timer(current_rsp: u64) -> u64
    
    # handle_timer returns new RSP in RAX
    mov rsp, rax                   # Switch to new RSP (may be different for context switch)
    
    pop r15; ... pop rax           # Restore registers from (possibly new) stack
    test qword ptr [rsp + 8], 3    # Check CS & 3 again
    jz 3f
    swapgs                         # If returning to user, swap GS back
3:
    iretq                          # Restore RIP, CS, RFLAGS, RSP, SS
```

**KEY INSIGHT**: The interrupt frame's .rsp field contains the USER RSP value that was 
interrupted. This is a CPU-architectural fact: when privileged interrupt occurs, the 
interrupted mode's RSP is saved, not the kernel's RSP.

---

### Phase 2: Timer Handler (kernel/arch/arch-x86_64/src/exceptions.rs:1410-1516)

```rust
extern "C" fn handle_timer(current_rsp: u64) -> u64 {
    // current_rsp = address of InterruptFrame on kernel stack
    
    // ... tick accounting, performance monitoring ...
    
    // Call the scheduler
    let new_rsp = unsafe {
        let cb_ptr = addr_of!(SCHEDULER_CALLBACK);
        if let Some(callback) = *cb_ptr {
            callback(current_rsp)  // Pass &InterruptFrame
        } else {
            current_rsp            // No context switch
        }
    };
    
    return new_rsp;  // Return new RSP (may be changed if context switch)
}
```

---

### Phase 3: scheduler_tick() - BUILD CURRENT CONTEXT (kernel/src/scheduler.rs:570-695)

```rust
pub fn scheduler_tick(current_rsp: u64) -> u64 {
    let frame = unsafe {
        &*(current_rsp as *const InterruptFrame)
    };
    
    // ⚠️ CRITICAL BUG HERE ⚠️
    let current_ctx = sched::TaskContext {
        rip: frame.rip,           // ✓ correct: kernel RIP when interrupted
        rsp: frame.rsp,           // ✗ BUG: user RSP from interrupt context
        rflags: frame.rflags,
        rax: frame.rax,
        // ... all other registers from frame ...
        cs: frame.cs,             // 0x08 or 0x23
        ss: frame.ss,
        fs_base: /* read from IA32_FS_BASE MSR */,
        gs_base: /* read from IA32_KERNEL_GS_BASE MSR */,
    };
    
    // context_switch_transaction: atomically:
    // 1. Save current_ctx into current_pid's Task.context
    // 2. Load next_pid's context into SwitchInfo
    // 3. Update scheduler state
    let switch_info = match sched::context_switch_transaction(
        current_pid,
        next_pid,
        current_ctx,      // ✗ Contains USER RSP!
        kernel_preempt_ok,
    ) {
        Some(info) => info,
        None => return current_rsp,  // Lock contended, retry next tick
    };
    
    // ... more setup ...
}
```

**MEMORY FLOW**:
- Task was in syscall (kernel mode, CS=0x08)
- Timer interrupt occurred
- CPU pushed: RIP, CS, RFLAGS, user_RSP, SS
- Handler saves all regs → InterruptFrame
- frame.rsp = 0x7ffff... (user stack pointer)
- This value is saved into TaskContext.rsp ✗

---

### Phase 4: context_switch_transaction() (kernel/sched/sched/src/core.rs:1125-1210)

```rust
pub fn context_switch_transaction(
    old_pid: Pid,
    new_pid: Pid,
    old_ctx: crate::task::TaskContext,  // Contains frame.rip, frame.rsp, ...
    kpo_value: bool,
) -> Option<SwitchInfo> {
    let cpu = this_cpu();
    
    let result = try_with_rq(cpu, |rq| {
        // Save outgoing task's context
        if let Some(old_task) = rq.get_task_mut(old_pid) {
            old_task.context = old_ctx;  // ✗ Saves user RSP into context!
            old_task.kernel_preempt_ok = kpo_value;
        }
        
        // Collect incoming task's info
        let new_task = rq.get_task(new_pid)?;
        
        // Validation: is context schedulable?
        if !new_task.context.is_schedulable() {
            return None;  // rip or rsp is 0
        }
        
        // BUILD SwitchInfo with next task's saved context
        let info = SwitchInfo {
            new_cr3: new_task.pml4_phys.as_u64(),
            new_rip: new_task.context.rip,
            new_rsp: new_task.context.rsp,  // ← kernel stack RSP from LAST switch
            new_fs_base: new_task.context.fs_base,
            new_gs_base: new_task.context.gs_base,
            new_kernel_stack: new_task.kernel_stack,
            new_kernel_stack_size: new_task.kernel_stack_size,
            new_kpo: new_task.kernel_preempt_ok,
            new_ctx: new_task.context,
        };
        
        // Re-enqueue old task, dequeue new, set rq.curr
        // ...
        
        Some(info)
    });
    
    result.flatten()
}
```

---

### Phase 5: scheduler_tick() - BUILD NEXT INTERRUPT FRAME (kernel/src/scheduler.rs:722-935)

```rust
pub fn scheduler_tick(current_rsp: u64) -> u64 {
    // ... prior code ...
    
    let switch_info = match sched::context_switch_transaction(...) {
        Some(info) => info,
        None => return current_rsp,
    };
    
    let next_ctx = switch_info.new_ctx;  // Incoming task's TaskContext
    let kernel_stack_top = {
        let ks_virt = phys_to_virt(switch_info.new_kernel_stack);
        ks_virt.as_u64() + switch_info.new_kernel_stack_size as u64
    };
    
    // Update kernel RSP for next syscall
    unsafe { arch::syscall::set_kernel_stack(kernel_stack_top); }
    arch::gdt::set_kernel_stack(kernel_stack_top);
    
    // Switch page tables
    unsafe { core::arch::asm!("mov cr3, {}", in(reg) switch_info.new_cr3); }
    
    // Restore FS base for TLS
    // ...
    
    // ⚠️ CRITICAL: Determine where to place new interrupt frame ⚠️
    let cs = if next_ctx.cs != 0 { next_ctx.cs } else { 0x23 };
    let ss = if cs == 0x08 { 0x10 } else { 0x1B };
    let is_kernel_mode = cs == 0x08;
    
    let frame_size = core::mem::size_of::<InterruptFrame>() as u64;
    let raw_ptr = if is_kernel_mode {
        // KERNEL MODE: Place frame below saved RSP
        next_ctx.rsp - frame_size  // ✗ BUG: Uses saved user RSP!
    } else {
        // USER MODE: Use top of kernel stack
        kernel_stack_top - frame_size
    };
    
    let new_frame_ptr = (raw_ptr & !7u64) as *mut InterruptFrame;
    
    // WRITE THE FRAME
    unsafe {
        (*new_frame_ptr).ss = ss;
        (*new_frame_ptr).rsp = next_ctx.rsp;      // ✗ User RSP written to frame!
        (*new_frame_ptr).rflags = next_ctx.rflags | 0x200;
        (*new_frame_ptr).cs = cs;
        (*new_frame_ptr).rip = next_ctx.rip;
        (*new_frame_ptr).rax = next_ctx.rax;
        // ... all other registers ...
    }
    
    new_frame_ptr as u64  // Return address of new frame
}
```

**THE BUG**:
- Line 898-900: `raw_ptr = next_ctx.rsp - frame_size` for kernel-mode
- `next_ctx.rsp` contains the USER stack pointer (from old interrupt frame)
- This calculates an address in USER space, not kernel stack!
- The new frame gets written to arbitrary user memory
- iretq tries to restore from the wrong memory location
- RIP gets whatever value is at that address (possibly the original RSP value itself)

---

### Phase 6: iretq Restoration (kernel/arch/arch-x86_64/src/exceptions.rs:443)

```asm
iretq:
    # Stack is now: [new_frame from scheduler]
    # Pops: RIP, CS, RFLAGS, RSP, SS (in that order)
    # If frame was written to wrong address due to line 898-900 bug,
    # the popped RIP is garbage
```

---

## WHY RSP VALUE ENDS UP IN RIP

The user RSP value (0x7ffffd15e990) appears in RIP because:

1. Scheduler writes frame at wrong address (based on old user RSP)
2. The memory at that location may contain:
   - Another copy of the user RSP (from syscall context or user stack)
   - Garbage that happens to match the RSP value
   - The actual next_ctx.rip got overwritten

**Example scenario**:
- User RSP = 0x7ffffd15e990
- Frame should be written to kernel stack
- Instead written to (0x7ffffd15e990 - frame_size)
- User stack at that location happens to contain 0x7ffffd15e990
- iretq pops RIP = 0x7ffffd15e990 → page fault in user space

---

## THE FIX NEEDED

For kernel-mode context switches, must track the ACTUAL kernel stack RSP where 
the interrupt frame was delivered, not the user RSP from the interrupt context.

Options:
1. **Store kernel stack RSP separately** in TaskContext when saving interrupted kernel-mode task
2. **Calculate it from kernel_stack_top** if we know the frame size
3. **Use a different frame placement strategy** that doesn't depend on saved RSP for kernel-mode
