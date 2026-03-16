# Session: Implement Linux-style preempt_count to replace kpo boolean hacks — per-CPU atomic counter, KernelMutex, heap migration, scheduler integration

| Field | Value |
|-------|-------|
| ID | `854dd3542e47` |
| Agent | claude-code |
| Model | claude-opus-4-6 |
| Project | oxide-os |
| Branch | - |
| Status | completed |
| Started | 2026-03-04T15:56:55.666601+00:00 |
| Ended | 2026-03-04T16:11:10.152001+00:00 |
| Compactions | 0 |

## Summary

Implemented Linux-style preempt_count to replace kpo boolean hacks. Per-CPU AtomicI32 counter in arch-x86_64, KernelMutex wrapper in os_core/sync.rs, heap allocator migrated from spin::Mutex to KernelMutex, scheduler updated to check preemptable() and save/restore counter across context switches. Removed exec VFS lookup kpo hack that caused Build 67 deadlocks.

## Session Summary

**Outcome:** Clean build, boots to login prompt in QEMU with zero stalls, zero deadlocks, zero panics. 230+ context switches during boot without issues.

**Decisions:**

- preempt_count is per-CPU AtomicI32 (not per-task) for ISR-safe lock-free access
- KernelMutex uses fn-pointer callbacks to avoid os_core depending on arch crate
- backward-compat aliases preserve all 56 existing kpo call sites
- PreemptToken struct ensures correct drop ordering (spinlock released before preempt_enable)
- Only heap allocator migrated to KernelMutex as first consumer — gradual migration for others

**Files Modified:**

- kernel/arch/arch-x86_64/src/lib.rs
- kernel/core/os_core/src/sync.rs
- kernel/core/os_core/src/lib.rs
- kernel/mm/mm-heap/src/hardened.rs
- kernel/mm/mm-heap/src/lib.rs
- kernel/src/init.rs
- kernel/src/scheduler.rs
- kernel/sched/sched/src/core.rs
- kernel/src/process.rs
- docs/agents/preempt-count-model.md
- CLAUDE.md

**Next Session Hints:** Gradually migrate other spin::Mutex users to KernelMutex where they're reachable from timer ISR. The oxide-test page fault (COW write to read-only page at 0x46ebe0) is a pre-existing issue unrelated to preemption changes. The PML4 corruption for pid=7 is also pre-existing.
