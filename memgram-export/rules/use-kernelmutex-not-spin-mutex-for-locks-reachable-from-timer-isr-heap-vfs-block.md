# Use KernelMutex (not spin::Mutex) for locks reachable from timer ISR — heap, VFS, block I/O

🔴 critical | ✅ do

| Field | Value |
|-------|-------|
| ID | `50ec05f83015` |
| Reinforced | ×1 |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-04T16:10:44.907932+00:00 |
| Keywords | KernelMutex, preempt_count, preemption, deadlock, heap, spin::Mutex, timer ISR |
| Session | [854dd3542e47](../sessions/implement-linux-style-preempt-count-to-replace-kpo-boolean-hacks-per-cpu-atomic.md) |

## Details

KernelMutex (os_core::sync::KernelMutex) wraps spin::Mutex with preempt_disable/enable. The scheduler timer ISR checks arch::preemptable() (preempt_count == 0) before context switching. If a task is preempted while holding a raw spin::Mutex, the next task scheduled on the same CPU will deadlock trying to acquire the same lock. KernelMutex prevents this by making the holder non-preemptable. The heap allocator was the first consumer — Build 67 deadlocked because exec VFS lookup was marked preemptable (kpo=true) while heap alloc held spin::Mutex.
