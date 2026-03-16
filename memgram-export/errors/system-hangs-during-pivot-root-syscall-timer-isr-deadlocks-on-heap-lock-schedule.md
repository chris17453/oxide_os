# Error: System hangs during pivot_root syscall — timer ISR deadlocks on heap lock. sched

| Field | Value |
|-------|-------|
| ID | `bd622b9ad914` |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-06T23:06:29.827878+00:00 |
| Keywords | isr, heap, deadlock, timer, schedulertick, allpids, vec, allocation, pivotroot |
| Files | `kernel/src/scheduler.rs`, `kernel/sched/sched/src/runqueue.rs`, `kernel/sched/sched/src/core.rs` |

## Error

System hangs during pivot_root syscall — timer ISR deadlocks on heap lock. scheduler_tick() called all_pids() which allocates Vec on heap. When interrupted code holds heap lock, ISR spins forever with interrupts disabled.

## Cause

scheduler_tick() (timer ISR) → all_pids() → Vec::with_capacity(4) → HEAP_ALLOCATOR.lock() → deadlock. The interrupted code (pivot_root → alloc::format!) was holding the heap lock. Timer ISR runs with interrupts disabled, so it spins forever.

## Fix

Added RunQueue::count_running() (zero-alloc counter) and sched::count_nr_running() to replace the allocating all_pids().filter().count() pattern in scheduler_tick. ISR code must NEVER allocate from the heap.
