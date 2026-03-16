# Scheduler RunQueue uses flat slot array (not BTreeMap) — O(1) task lookup via PID_TO_SLOT global

🔴 critical | ✅ do

| Field | Value |
|-------|-------|
| ID | `d4714f969218` |
| Reinforced | ×1 |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-04T22:50:30.810759+00:00 |
| Keywords | scheduler, RunQueue, flat-array, PID_TO_SLOT, BTreeMap, O(1), slot, context-switch |
| Files | `kernel/sched/sched/src/runqueue.rs` |
| Session | [beaad52bec1e](../sessions/fix-syscall-number-bugs-in-oxide-test-commit-vma-kernelmutex-work-implement-flat.md) |

## Details

RunQueue.tasks was replaced from BTreeMap&lt;Pid, Task&gt; to Vec&lt;Option&lt;Task&gt;&gt; (fixed 256 slots) with a free-stack allocator and global PID_TO_SLOT: [AtomicU16; 4096] for O(1) lookup. Zero heap allocations on context switch hot path. Slot accessors slot_get(pid)/slot_get_mut(pid) validate PID matches to defend against stale mappings. The borrow checker requires caching self.clock before slot_get_mut and splitting CFS update_min_vruntime into a separate scope after the mutable borrow ends.
