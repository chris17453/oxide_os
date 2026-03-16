# Every AP must have a proper idle Task in its run queue BTreeMap

🔴 critical | ✅ do

| Field | Value |
|-------|-------|
| ID | `26c76983c887` |
| Reinforced | ×1 |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-02T20:43:00.576889+00:00 |
| Condition | When initializing AP CPUs in SMP boot |
| Keywords | SMP, AP, idle, Task, run queue, BTreeMap, add_task_to_cpu |
| Files | `kernel/src/smp_init.rs`, `kernel/sched/sched/src/core.rs` |

## Details

Each AP must create a Task for PID 0 (idle) on its own run queue via sched::add_task_to_cpu(). Without it, pick_next_task returns idle PID 0 but context_switch_transaction fails (get_task(0)→None), corrupting rq.curr. The idle Task must have cs=0x08, ss=0x10, rflags=0x202. Use add_task_to_cpu (not add_task) because select_task_rq would route to CPU 0 based on last_cpu=0 default.
