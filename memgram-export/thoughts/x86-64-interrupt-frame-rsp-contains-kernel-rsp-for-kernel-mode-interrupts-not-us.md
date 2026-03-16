# x86-64 interrupt frame.rsp contains KERNEL RSP for kernel-mode interrupts — NOT user RSP

📌 Pinned

| Field | Value |
|-------|-------|
| ID | `19ded98ee7fa` |
| Type | observation |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-03T10:26:52.528711+00:00 |
| Accessed | 0 times |
| Keywords | x86-64, interrupt-frame, frame-rsp, kernel-mode, context-switch, frame-placement, scheduler |
| Session | [de10f3d29fd9](../sessions/continue-scheduler-context-switch-hardening-verify-completion-state.md) |

## Content

CORRECTION of previous flawed analysis (archived thoughts b3836285ac62, 0702a227c10c, 534e65a5bc4c).

## The Truth About frame.rsp in x86-64 Long Mode

For a same-privilege interrupt (ring 0 → ring 0, no IST):
1. CPU uses the CURRENT stack (no TSS.RSP0 lookup)
2. CPU pushes: SS, RSP, RFLAGS, CS, RIP onto the current stack
3. The pushed RSP is the RSP value BEFORE the interrupt — i.e., the kernel stack pointer

For a privilege-change interrupt (ring 3 → ring 0):
1. CPU loads RSP from TSS.RSP0 (kernel stack)
2. CPU pushes: SS(user), RSP(user), RFLAGS, CS(user=0x23), RIP(user) onto kernel stack
3. The pushed RSP is the USER RSP

Therefore:
- Kernel-mode interrupt: frame.rsp = KERNEL RSP ✓
- User-mode interrupt: frame.rsp = USER RSP ✓

## Frame Placement Math (scheduler.rs line 928-934)

For kernel-mode (cs=0x08):
  raw_ptr = next_ctx.rsp - frame_size
  = kernel_rsp_before_interrupt - sizeof(InterruptFrame)
  = exact location where the original interrupt frame was pushed ✓

For user-mode (cs=0x23):
  raw_ptr = kernel_stack_top - frame_size
  (ignores ctx.rsp, uses kernel stack top) ✓

Both are correct. No kernel_stack_rsp field needed.

## What Actually Caused the Previous Crash
BSP idle task had rsp=0 (TaskContext::default() was all-zeros before Phase 2 fix).
Frame placement: 0 - 160 = 0xFFFFFFFFFFFFFF60 (underflow) → wrote iretq frame to garbage memory.
Fixed by capturing boot_rsp via inline asm in scheduler::init().
