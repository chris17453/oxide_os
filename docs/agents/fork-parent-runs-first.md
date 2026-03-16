# Fork: Parent-Runs-First + CPU Distribution

## Rule
`kernel_fork()` MUST return `child_pid` to the parent through the normal syscall
return path. The child is added to a runqueue and scheduled by the next timer tick.

NEVER use `switch_to(child_pid)` + inline `sysretq` to jump directly into the child
from within the fork syscall.

## Why
The old "child-runs-first" implementation caused three fatal bugs:

1. **All tasks pile onto CPU#0** — `add_task_to_cpu(child, this_cpu())` always targets
   the CPU running the fork syscall (CPU#0 for init). With 4 CPUs, only CPU#0 runs
   user tasks while CPUs 1-3 idle in HLT.

2. **Parent starvation** — After fork's `sysretq` to the child, the parent sits in
   CPU#0's CFS tree. When the child execs getty → login → esh, those descendants
   consume CPU#0's timeslices. Later forked children (PIDs 5-8) never get scheduled.
   Init spawns 6 gettys but only 2 ever run.

3. **PARENT_CONTEXT global is SMP-unsafe** — A single `Mutex<Option<ParentContext>>`
   stores the parent's saved state. If two CPUs fork concurrently, one parent's
   context gets clobbered.

## The Fix

```rust
// Round-robin child across CPUs
let target_cpu = FORK_CPU_COUNTER.fetch_add(1, Relaxed) % num_cpus;
sched::add_task_to_cpu(child_task, target_cpu);

// Parent returns normally through syscall dispatch
child_pid as i64
```

No CR3 switch, no manual `sysretq`, no `PARENT_CONTEXT`, no kernel stack switching.
The scheduler's `context_switch_transaction` handles the child's first run via normal
`iretq` on whichever CPU picks it up.

## Linux Reference
Linux switched to parent-runs-first in 2.6.32 (commit sysctl_sched_child_runs_first
defaulting to 0). Same reasoning: child-runs-first causes thundering-herd wakeup
storms and SMP imbalance.
