# Error: Only 2 of 6 VT gettys get exec'd. Init forks all 6 PIDs successfully (confirmed 

| Field | Value |
|-------|-------|
| ID | `3e32e70e2de9` |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-13T19:39:46.557097+00:00 |
| Keywords | fork, getty, scheduler, vt, init, smp, cpuaffinity, starvation |
| Files | `kernel/src/process.rs`, `kernel/sched/sched/src/core.rs`, `kernel/sched/sched/src/runqueue.rs`, `userspace/system/init/src/main.rs` |
| Session | [3bfe90c6680d](../sessions/fix-multi-vt-terminal-spawning-only-1-vt-gets-a-working-shell-debug-and-fix-the.md) |

## Error

Only 2 of 6 VT gettys get exec'd. Init forks all 6 PIDs successfully (confirmed by serial trace) but child PIDs 5-8 never get scheduled — they never execute their child path (setsid/open/exec). Only the first 2 children (PIDs 3,4) actually run.

## Cause

Fork's switch_to(child_pid) + sysretq immediately runs the child on the current CPU. The parent is re-enqueued. But with 4 CPUs idle (only CPU#0 running user tasks), forked children land on CPU#0's runqueue. After 2 rapid forks, the children exec getty which forks login→esh, consuming the CPU. Later children never get picked by the scheduler because all work is on CPU#0 and the CFS tree may be starved by earlier children's descendants.

## Fix

Multiple potential fixes: (1) Distribute forked children across CPUs via load balancing, (2) Ensure init gets enough timeslices to complete its fork loop before children consume the CPU, (3) Check if scheduler's add_task_to_cpu always targets CPU#0 and spread the load.
