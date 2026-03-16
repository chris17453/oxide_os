# Kernel preemption model: hybrid Linux-style — never preempt without kpo, emergency timeout at 500 ticks

🔴 critical | ✅ do

| Field | Value |
|-------|-------|
| ID | `3a0eeddec88c` |
| Reinforced | ×1 |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-04T15:11:29.039352+00:00 |
| Keywords | scheduler, preemption, kpo, kernel_preempt_ok, spinlock, deadlock, timer-isr |
| Session | [284be71ff04f](../sessions/replace-kpo-grace-period-forced-preemption-with-linux-style-never-preempt-kernel.md) |

## Details

The scheduler timer ISR uses a hybrid preemption model:
1. in_kernel && !kpo → DON'T preempt (spinlock holders). Just tick vruntime and return.
2. in_kernel && kpo → can preempt (blocking I/O, poll, nanosleep).
3. userspace → always preemptible via CFS.
4. SAFETY NET: if a task is in kernel mode without kpo for 500+ ticks (5 seconds), force-preempt. No spinlock is held for 5 seconds — this catches genuinely stuck drivers (e.g., virtio-blk polling without kpo).

Key: kpo MUST be set for all blocking kernel paths (sys_read, sys_write, sys_open VFS lookup, exec file read, poll, nanosleep). Setting kpo globally for ALL syscalls causes heap lock convoys on single-CPU systems (preempted heap lock holder → all other tasks spin → permanent stall).
