# RSP-to-RIP clobber root cause analysis — register flow in context switch

📌 Pinned 🗄️ Archived

| Field | Value |
|-------|-------|
| ID | `b3836285ac62` |
| Type | observation |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-03T01:59:20.672994+00:00 |
| Accessed | 0 times |
| Keywords | context-switch, RSP-RIP-clobber, interrupt-frame, kernel-mode, scheduler_tick, frame-placement, register-corruption |

## Content

CRITICAL FINDING: The crash showing RSP value 0x7ffffd15e990 in RIP is caused by interleaving between SYSCALL context (rsp, rip on kernel stack) and SCHEDULER context (rsp, rip in TaskContext).

## Register Flow Analysis:

### 1. SYSCALL Path (interrupts kernel-mode task):
- Timer ISR fires on kernel-mode task (CS=0x08) in syscall
- Interrupt frame pushed: RIP, CS, RFLAGS, RSP, SS (from CPU)
- Timer handler calls scheduler_tick(current_rsp=&frame)
- scheduler_tick reads the INTERRUPT FRAME and builds TaskContext from it:
  - current_ctx.rip = frame.rip (the kernel RIP when interrupted)
  - current_ctx.rsp = frame.rsp (user RSP, NOT the kernel stack pointer!)

### 2. The Critical Bug:
In scheduler_tick (lines 671-695), when interrupted in kernel mode:
```rust
let current_ctx = sched::TaskContext {
    rip: frame.rip,      // ✓ kernel RIP correct
    rsp: frame.rsp,      // ✗ BUG: This is USER RSP, not kernel RSP!
    ...
}
```

The interrupt frame's .rsp is always the USER RSP that was active when interrupt occurred. For a task preempted mid-syscall, this is the user's stack pointer value from BEFORE syscall entry, not the actual kernel stack RSP that the interrupt frame was pushed to.

### 3. How it Manifests:
1. Task makes syscall, enters kernel mode (RSP = kernel stack)
2. Timer interrupt fires, saves INTERRUPT FRAME on top of kernel stack
3. scheduler_tick() reads frame.rsp (user RSP from syscall context)
4. Saves frame.rsp into TaskContext.rsp
5. Later, when task is rescheduled, context_switch_transaction returns SwitchInfo
6. scheduler_tick() calculates frame placement (line 898):
   - For kernel-mode tasks: raw_ptr = next_ctx.rsp - frame_size
   - This uses the SAVED USER RSP, not kernel RSP!
7. Writes new interrupt frame to wrong memory location
8. iretq pops RIP from wrong address → RIP gets garbage (or RSP value if unlucky)

### 4. Why it looks like RSP→RIP:
When the frame is written to an arbitrary user stack address and then iretq tries to restore:
- The popped value might coincidentally be the user's stack pointer value (0x7ffffd15e990)
- This happens because user's stack data gets scrambled or the memory contains the RSP value

### 5. The Real Issue:
For kernel-mode context switches, the frame placement logic at line 898-904 is flawed:
- Assumes next_ctx.rsp holds the kernel stack pointer for kernel-mode tasks
- But that value was copied from the interrupt frame.rsp (user stack pointer)
- The ACTUAL kernel RSP where the interrupt was delivered is lost after interrupt handling

The correct kernel RSP for a kernel-mode interrupted task should be:
- The address just above the interrupt frame on the kernel stack
- NOT the user RSP that was in the interrupted context

## Stack Layout When Interrupted During Syscall:

```
Kernel Stack (grows down):
[kernel stack top] ← kernel_stack_top (correct, used for user-mode frame placement)
[kernel data]
[saved user context from syscall prologue - CRITICAL]
...
[interrupt frame] ← current_rsp passed to scheduler_tick
[rip][cs][rflags][rsp=user_rsp][ss] ← CPU-pushed frame elements

Then:
current_rsp = &interrupt_frame
frame.rsp = user_rsp (NOT kernel stack location)
Saved to TaskContext.rsp = user_rsp

Later when rescheduled:
Tries to write new frame at: user_rsp - frame_size
This is USER address space, not kernel stack!
```
