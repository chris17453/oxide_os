# Error: Only 2-3 of 6 VTs work randomly on each boot. Tasks assigned to certain CPUs nev

| Field | Value |
|-------|-------|
| ID | `dcd3bf3eb274` |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-14T00:36:15.045447+00:00 |
| Keywords | scheduler, idletrysteal, onrq, cfs, taskloss, vt, smp, workstealing |
| Files | `kernel/sched/sched/src/runqueue.rs`, `kernel/sched/sched/src/core.rs`, `kernel/src/scheduler.rs` |

## Error

Only 2-3 of 6 VTs work randomly on each boot. Tasks assigned to certain CPUs never get scheduled.

## Cause

idle_try_steal steals tasks between CPUs. remove_task dequeues from CFS but leaves on_rq=true on the Task struct. When the stolen task is add_task'd to the thief's RQ, enqueue_task checks if task.on_rq { return } — sees true from the old RQ — silently skips CFS insertion. Task sits in slot forever, never scheduled. Which tasks get stolen depends on timing → random VTs.

## Fix

One-line fix in kernel/sched/sched/src/runqueue.rs remove_task(): set task.on_rq = false before returning the Task. Also changed context_switch_transaction to use with_rq (blocking) instead of try_with_rq to prevent pick-then-fail livelock.
