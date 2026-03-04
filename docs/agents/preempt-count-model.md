# Linux-Style preempt_count Model

## Rule
The kernel uses a per-CPU atomic `preempt_count` counter (not a boolean flag) to control preemption. The scheduler only preempts when `preempt_count == 0`.

## Mechanism
- `arch::preempt_disable()` — increments counter (called by `KernelMutex::lock()`)
- `arch::preempt_enable()` — decrements counter (called by `KernelMutexGuard::drop()`)
- `arch::preemptable()` — returns `true` when counter == 0
- `arch::get_preempt_count()` / `arch::set_preempt_count()` — save/restore across context switches

## KernelMutex
`os_core::sync::KernelMutex<T>` wraps `spin::Mutex<T>` with automatic preempt_disable/enable. Use it for any lock that might be held when a timer ISR fires (heap allocator, VFS locks, etc.). The preemption hooks are registered via `os_core::register_preempt_hooks()` during early boot.

## Context Switch
The scheduler saves the outgoing task's `preempt_count` in `Task.preempt_count` and restores the incoming task's value. This preserves lock nesting depth across context switches.

## Legacy Compatibility
Old `allow_kernel_preempt()` / `disallow_kernel_preempt()` APIs still work as aliases:
- `allow_kernel_preempt()` sets counter to 0
- `disallow_kernel_preempt()` sets counter to 1 (idempotent)
- `is_kernel_preempt_allowed()` returns `preemptable()`

## Key Files
- `kernel/arch/arch-x86_64/src/lib.rs` — PREEMPT_COUNT array + API
- `kernel/core/os_core/src/sync.rs` — KernelMutex, preempt hook registration
- `kernel/mm/mm-heap/src/hardened.rs` — heap uses KernelMutex
- `kernel/mm/mm-heap/src/lib.rs` — heap uses KernelMutex
- `kernel/src/scheduler.rs` — reads preemptable() in timer ISR
- `kernel/sched/sched/src/core.rs` — context_switch_transaction saves/restores counter

## Why Not Boolean
A boolean loses information. If a task holds 2 nested KernelMutex locks and gets preempted, the boolean can only say "not preemptable." On resume, it can't distinguish "holding 1 lock" from "holding 3 locks." The counter preserves exact nesting depth.

## Migration Path
- Heap allocator: migrated to KernelMutex (Build 68)
- Other spin::Mutex users: gradual migration in future PRs
- Manual kpo call sites: still work through backward-compat aliases, clean up gradually
