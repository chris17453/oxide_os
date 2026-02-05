# Syscall Return Reschedule Rule

**Author:** GraveShift
**Context:** `kernel/src/scheduler.rs` → `check_signals_on_syscall_return()`

## Problem

The timer ISR (`handle_timer`) skips context switches for tasks in kernel
mode (CS=0x08) unless `kernel_preempt_ok` is set. Only the idle loop and
explicit blocking syscalls (poll, nanosleep) set that flag.

A user-mode process that hammers fast syscalls (write/fork/open in a loop)
spends nearly 100% of its wall time inside kernel handlers. The timer
always catches it with CS=0x08, sets `need_resched` in the RunQueue, but
never performs the switch. The task monopolises its CPU indefinitely.

## Rule

**Every syscall exit path must check `sched::need_resched()` and yield
before returning to user mode.**

The check lives in `check_signals_on_syscall_return()` (the signal-check
callback invoked by the sysret asm epilogue). Before examining pending
signals, the function:

1. Reads `sched::need_resched()`.
2. If true, saves `SYSCALL_USER_CONTEXT` to the kernel stack (it's a
   global — another task's `syscall_entry` would clobber it while we sleep).
3. Sets `kernel_preempt_ok`, does `sti; hlt`. The next timer interrupt
   sees the flag and performs the context switch.
4. On resume: `cli` (restore the invariant expected by the sysret asm),
   write the saved context back to `SYSCALL_USER_CONTEXT`.

## Why not preempt in kernel mode?

Arbitrary kernel preemption is unsafe for OXIDE right now — many kernel
paths hold spin-locks, reference `static mut` globals, or assume
single-threaded access. The syscall return point is a natural preemption
boundary: all kernel work is complete, no locks are held, and the task is
about to sysret anyway.

## SYSCALL_USER_CONTEXT is global (not per-task)

This is a pre-existing design constraint. The single global works because
only one task at a time is in a syscall on a given CPU. The resched yield
temporarily violates this (another task runs and does syscalls), so the
save/restore is mandatory.

## Files

| File | What to check |
|------|---------------|
| `kernel/src/scheduler.rs` | `check_signals_on_syscall_return()` — the resched check |
| `kernel/arch/arch-x86_64/src/syscall.rs` | `syscall_signal_check()`, `SYSCALL_USER_CONTEXT` |
| `kernel/arch/arch-x86_64/src/exceptions.rs` | `handle_timer` — kernel-mode preemption gate |
