# Timer ISR (scheduler_tick) must NEVER call functions that allocate from the heap

📌 Pinned | 🔴 critical | ❌ dont

| Field | Value |
|-------|-------|
| ID | `0c270f73cbeb` |
| Reinforced | ×1 |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-06T23:06:43.467523+00:00 |
| Condition | When adding or modifying code called from scheduler_tick, handle_timer, or any ISR path |
| Keywords | isr, heap, timer, schedulertick, deadlock, vec, allocation |
| Files | `kernel/src/scheduler.rs`, `kernel/sched/sched/src/runqueue.rs`, `docs/agents/isr-no-heap-allocation.md` |

## Details

scheduler_tick() runs inside the timer ISR with interrupts disabled. Any heap allocation (Vec, String, format!, Box) will try to lock HEAP_ALLOCATOR. If the interrupted code was holding that lock, the ISR deadlocks forever — the system hangs with no visible crash or error. This was the root cause of the pivot_root hang: loadavg sampling called all_pids() which created a Vec. Fix: use zero-alloc alternatives like count_running() that iterate without collecting.
