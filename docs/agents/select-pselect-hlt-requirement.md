# sys_select / sys_pselect6 MUST Use HLT, Not spin_loop

**Author:** GraveShift
**Status:** ACTIVE RULE — do not revert to spin_loop

## Problem

`sys_select` and `sys_pselect6` used `core::hint::spin_loop()` (x86 PAUSE instruction)
as their wait mechanism. PAUSE is a yield hint that burns ~10 cycles per iteration —
it's a busy-spin that consumes 100% CPU in kernel mode with `kpo=0`, making the task
completely unpreemptable by the scheduler.

Any process calling `select()` or `pselect6()` became an unkillable CPU hog.

## Fix

Replace `spin_loop()` with the same HLT pattern used by `sys_poll`:

```rust
arch_x86_64::allow_kernel_preempt();
unsafe {
    core::arch::asm!("sti", "hlt", options(nomem, nostack));
}
arch_x86_64::disallow_kernel_preempt();
```

**Location:** `kernel/syscall/syscall/src/poll.rs`, inside the wait loops of
`sys_select` and `sys_pselect6`.

## Rules

1. **ALL blocking syscall wait loops MUST use `allow_kernel_preempt()` + `sti; hlt`**
   — never `spin_loop()`, never bare `hlt` without `sti`, never `sti` and `hlt` in
   separate asm blocks (race condition: interrupt between them → extra tick delay).

2. **`sti` and `hlt` MUST be in the same `asm!()` block.** If separated, an interrupt
   can fire between them, handle it, return, and the CPU hits HLT waiting for the
   NEXT interrupt — one extra tick of unnecessary latency.

3. **`allow_kernel_preempt()` MUST be called before HLT.** Without it, the timer ISR
   sees `kpo=0` and refuses to context-switch, defeating the entire purpose of yielding.

4. **`disallow_kernel_preempt()` MUST be called after HLT returns.** The task resumes
   in kernel mode and will check conditions / return to userspace. kpo must be cleared
   so the syscall dispatch path has correct preemption state.

## Pattern: Every Blocking Syscall Wait Loop

```rust
loop {
    // Check if condition is met → return
    // Check timeout → return 0
    // Check signals → return EINTR

    // Yield CPU properly:
    arch_x86_64::allow_kernel_preempt();
    unsafe {
        core::arch::asm!("sti", "hlt", options(nomem, nostack));
    }
    arch_x86_64::disallow_kernel_preempt();
}
```

## Affected Syscalls

| Syscall | File | Status |
|---------|------|--------|
| sys_poll | poll.rs | ✅ Always used HLT correctly |
| sys_ppoll | poll.rs | ✅ Delegates to sys_poll |
| sys_select | poll.rs | ✅ Fixed (was spin_loop) |
| sys_pselect6 | poll.rs | ✅ Fixed (was spin_loop) |
| sys_nanosleep | time.rs | ✅ Uses sleep queue + HLT |
