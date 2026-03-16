# remove_task MUST clear on_rq=false before returning Task — work stealing depends on it

🔴 critical | ✅ do

| Field | Value |
|-------|-------|
| ID | `63a03319c24a` |
| Reinforced | ×1 |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-14T00:36:26.229250+00:00 |
| Condition | When modifying remove_task or any code that moves Tasks between RunQueues |
| Keywords | scheduler, removetask, onrq, workstealing, cfs, enqueue |
| Files | `kernel/sched/sched/src/runqueue.rs` |

## Details

When remove_task() takes a Task out of a RunQueue (dequeues from CFS/RT tree, frees the slot), it MUST set task.on_rq = false before returning. idle_try_steal calls remove_task on the victim CPU then add_task on the thief CPU. add_task calls enqueue_task which checks on_rq — if still true, it silently skips CFS insertion and the task is permanently lost (in the slot but invisible to the scheduler). This was the root cause of the "random 2-3 VTs" bug that took hours to diagnose.
