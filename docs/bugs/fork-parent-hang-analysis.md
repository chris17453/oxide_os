# Fork Parent Hang — Root Cause Analysis

## Symptom
After fork, the parent (shell) NEVER resumes. Programs execute fine but the shell hangs forever after they exit. "Context switches: 2, Preemptions: 0" in perf output.

## Root Cause: CFS tick preemption check uses wrong threshold

**File:** `kernel/sched/sched/src/fair.rs:412` (dead code — not called, but has the bug)
**File:** `kernel/sched/sched/src/runqueue.rs:552-561` (LIVE code — this is the real check)

### The bug in fair.rs (already fixed, but dead code)
```rust
// OLD (broken):
return delta >= TICK_NS && rq.nr_running() > 1;
// NEW (fixed):
return delta >= TICK_NS && rq.nr_running() > 0;
```

`nr_running` counts tasks **on the CFS tree (queued/waiting)**. The currently running task is NOT on the tree (`on_rq=false` after `pop_next_task`). So `nr_running=1` means one task is waiting. The old check `> 1` required TWO waiting tasks — meaning a single parent waiting for its forked child would never trigger preemption.

**NOTE:** `FairSchedClass::tick()` is never actually called anywhere. The RunQueue's `scheduler_tick` handles CFS ticks inline.

### The live preemption check (runqueue.rs:552-561)
```rust
if let Some(next_pid) = self.cfs_rq.pick_next() {
    if next_pid != curr_pid {
        let curr_vr = self.slot_get(curr_pid).map(|t| t.vruntime).unwrap_or(0);
        let next_vr = self.slot_get(next_pid).map(|t| t.vruntime).unwrap_or(0);
        if next_vr + 1_000_000 < curr_vr {
            self.need_resched = true;
            return true;
        }
    }
}
```

This check SHOULD work — it picks the lowest-vruntime task from the CFS tree and compares with current. After a few ticks, the child's vruntime exceeds the parent's and `need_resched` fires. **If this isn't triggering, investigate:**

1. Is `try_with_rq` failing (lock contention) every tick? → `scheduler_tick_ex` returns `None` → treated as false
2. Is the parent actually on the CFS tree? Check `cfs_rq.pick_next()` returns parent
3. Are vruntimes correct? Child starts at `min_vruntime - 6ms`, parent should be higher

## Secondary Issue: Cross-CPU task placement in fork

**File:** `kernel/src/process.rs:526`

```rust
sched::add_task(child_task);       // line 526 — uses select_task_rq()
sched::switch_to(child_pid);       // line 532 — runs on this_cpu()
```

`add_task` calls `select_task_rq` which picks CPU based on `task.last_cpu` (default 0 for new tasks). If fork runs on CPU X != 0:
- Child's Task goes to CPU 0's RQ
- `switch_to(child_pid)` runs on CPU X — tries to dequeue child from CPU X (no-op, child not there)
- `rq.set_curr(Some(child_pid))` on CPU X — current is a PID with no Task struct on this CPU
- `scheduler_tick` on CPU X → `slot_get(child_pid)` → None → returns false → **never triggers preemption**

**Fix:** Replace `sched::add_task(child_task)` with `sched::add_task_to_cpu(child_task, sched::this_cpu())` to ensure the child's Task struct is on the same CPU where fork runs.

**Currently all tasks likely live on CPU 0 (no migration), so this may not be the active bug, but it's a latent SMP correctness issue.**

## Other Findings

### PARENT_CONTEXT / CHILD_DONE — dead mechanism
- `PARENT_CONTEXT` is saved during fork (line 575) and cleared during exit (line 323)
- `CHILD_DONE` is set to false during fork (line 597) but **never set to true anywhere**
- This was probably an older parent-restore mechanism. Currently unused. Can be cleaned up.

### Exit path does set need_resched
- `kernel/src/process.rs:333`: `sched::set_need_resched()` IS called after exit
- `wake_parent` → `wake_up` also checks `should_preempt` and may set `need_resched`
- So the exit path SHOULD trigger a context switch on the next tick

## Recommended Investigation Order

1. **Add tracing to `runqueue.rs:scheduler_tick`** for the CFS branch — log `curr_pid`, `cfs_rq.pick_next()`, `nr_running`, `curr_vr`, `next_vr` to see if the preemption check is reached and what values it sees
2. **Check if `try_with_rq` succeeds** — if it always fails (returns None), the tick never runs
3. **Apply the `add_task_to_cpu` fix** for SMP correctness
4. **Verify `pick_next_task` finds the parent** after child exits and `need_resched` is set

## Changes Already Made

1. `kernel/sched/sched/src/fair.rs:412` — Changed `nr_running() > 1` to `nr_running() > 0` (correct semantics even though this code path is currently dead)

## Quick Reference: Fork Flow
```
kernel_fork() on CPU X:
  1. set_task_context(parent_pid, ctx_with_rax=child_pid)  — save parent for scheduler restore
  2. add_task(child_task)                                    — child goes to CPU 0 (last_cpu=0)
  3. switch_to(child_pid)                                    — re-enqueue parent, set child as current
  4. Change kernel stack + CR3 to child's
  5. sysretq to child (noreturn)                             — parent never returns from this function

  Parent restoration: scheduler timer tick → pick_next_task → context_switch_transaction → iretq
```
