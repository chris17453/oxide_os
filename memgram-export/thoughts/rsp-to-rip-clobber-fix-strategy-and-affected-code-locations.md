# RSP-to-RIP clobber: Fix strategy and affected code locations

📌 Pinned 🗄️ Archived

| Field | Value |
|-------|-------|
| ID | `534e65a5bc4c` |
| Type | idea |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-03T02:00:04.910217+00:00 |
| Accessed | 0 times |
| Keywords | RSP-RIP-clobber-fix, kernel-stack-rsp, TaskContext, frame-placement, kernel-preemption, scheduler-tick |

## Content

## AFFECTED CODE LOCATIONS

### Critical Files:
1. **kernel/src/scheduler.rs** (lines 670-904)
   - Line 674: `rsp: frame.rsp` ← BUG: saves user RSP
   - Line 898-900: Frame placement calculation for kernel-mode
   
2. **kernel/sched/sched/src/core.rs** (lines 1138-1140)
   - Saves TaskContext with corrupted RSP
   
3. **kernel/arch/arch-x86_64/src/exceptions.rs** (lines 386-445)
   - Timer ISR delivery (correct, not buggy)

---

## ROOT CAUSE SUMMARY

When a kernel-mode task is preempted by timer interrupt:
1. CPU pushes frame with INTERRUPTED mode's RSP (user RSP)
2. scheduler_tick() copies frame.rsp into current_ctx.rsp ✗
3. context_switch_transaction() saves this corrupted RSP to Task.context
4. Later, when task is rescheduled, frame placement uses this user RSP
5. New interrupt frame written to user space instead of kernel stack
6. iretq pops RIP from wrong location → garbage/crash

---

## FIX STRATEGY: Three Possible Approaches

### APPROACH A: Store Kernel Stack RSP Separately (BEST)
**Pros**: Clean, explicit, no calculation needed
**Changes**:
1. Add field to TaskContext: `kernel_stack_rsp: u64` (for kernel-mode frame placement)
2. In scheduler_tick line 674:
   - Keep rsp = frame.rsp (for user-mode, for resume point)
   - Add: kernel_stack_rsp = current_rsp + frame_size (actual kernel stack location)
3. In frame placement (line 898-900):
   - For kernel-mode: use next_ctx.kernel_stack_rsp instead of next_ctx.rsp

### APPROACH B: Use kernel_stack_top for All Frame Placement
**Pros**: Simpler, avoids new fields
**Cons**: Loses original kernel frame position, may break kernel-preempted-syscall resume
**Changes**:
1. Always place frame at: kernel_stack_top - frame_size
2. Don't differentiate between kernel/user mode for frame placement
3. Remove the is_kernel_mode branch at line 898-904

### APPROACH C: Store Frame Pointer in TaskContext
**Pros**: Tracks exact restoration point
**Cons**: Similar complexity to Approach A
**Changes**:
1. Add field to TaskContext: `frame_ptr: u64` (for kernel-mode only)
2. In scheduler_tick: kernel_stack_rsp = current_rsp + frame_size
3. Use this in frame placement logic

---

## RECOMMENDED: Approach A

### Implementation Plan:

#### Step 1: Modify TaskContext (kernel/sched/sched/src/task.rs)
```rust
pub struct TaskContext {
    pub rip: u64,
    pub rsp: u64,                  // User RSP (for user-mode exit point)
    pub rflags: u64,
    // ... existing fields ...
    pub cs: u64,
    pub ss: u64,
    pub fs_base: u64,
    pub gs_base: u64,
    pub kernel_stack_rsp: u64,     // NEW: Actual kernel stack RSP when interrupted in kernel
}
```

Default impl should set kernel_stack_rsp = 0 (lazy-loaded when needed)

#### Step 2: Update scheduler_tick() to Calculate kernel_stack_rsp
**File**: kernel/src/scheduler.rs, line ~671-695

```rust
let current_ctx = sched::TaskContext {
    rip: frame.rip,
    rsp: frame.rsp,                          // User RSP from interrupt
    rflags: frame.rflags,
    // ... all registers ...
    cs: frame.cs,
    ss: frame.ss,
    fs_base: current_fs_base,
    gs_base: current_gs_base,
    kernel_stack_rsp: if frame.cs == 0x08 { 
        current_rsp + core::mem::size_of::<InterruptFrame>() as u64
    } else {
        0  // Not meaningful for user-mode interrupts
    },
};
```

#### Step 3: Update Frame Placement Logic
**File**: kernel/src/scheduler.rs, line ~898-904

```rust
let frame_size = core::mem::size_of::<InterruptFrame>() as u64;
let raw_ptr = if is_kernel_mode {
    // KERNEL MODE: Use saved kernel stack RSP (where frame was originally)
    if next_ctx.kernel_stack_rsp != 0 {
        next_ctx.kernel_stack_rsp - frame_size
    } else {
        // Fallback if not available (shouldn't happen)
        kernel_stack_top - frame_size
    }
} else {
    // USER MODE: Use top of kernel stack
    kernel_stack_top - frame_size
};
```

#### Step 4: Update SwitchInfo (if needed)
**File**: kernel/sched/sched/src/core.rs

Currently SwitchInfo copies new_ctx fully, so kernel_stack_rsp will flow through automatically.

---

## VALIDATION CHECKLIST

After implementing fix:

1. **Compile without errors**
   - TaskContext has new field
   - kernel/src/scheduler.rs compiles
   - All TaskContext instantiations handled

2. **Test kernel-mode preemption**
   - syscall → preemption during handler → resume
   - idle task context switches
   - kernel_yield() syscall + preemption

3. **Test user-mode preemption**
   - Normal app context switches
   - Verify kernel_stack_rsp=0 for user frames doesn't affect anything

4. **Regression testing**
   - Original test cases that were failing
   - Page fault handler diagnostics
   - Multi-CPU scheduling

---

## KEY INSIGHT

The real issue is semantic confusion between two different RSP values:
- **frame.rsp**: The interrupted task's stack pointer (user RSP for ring3, kernel RSP for ring0)
- **kernel_stack_rsp**: The kernel stack pointer where THIS interrupt frame was pushed

For kernel-mode task preemption, saving frame.rsp loses information about where the frame actually lives on the kernel stack. The fix ensures we save both pieces of information and use the correct one for frame placement during rescheduling.
