# Scheduler Meta Fast-Path Rules

**Author:** GraveShift
**Scope:** `kernel/sched/sched/src/core.rs` — all scheduler query/mutation functions

## Problem

Every scheduler function that looked up a task by PID used to loop `0..num_cpus()`
calling `with_rq(cpu, ...)` — a **blocking spin lock acquire** on each CPU's run queue.

With 4 CPUs and `get_current_meta()` called on every syscall that touches process
metadata (open, read, write, getdents, stat, etc.), this meant **4 blocking lock
acquisitions per syscall** — even though the current task is ALWAYS on `this_cpu()`'s
run queue.

Combined with all user tasks living on CPU 0 (BSP), every task lookup was:
1. Lock CPU 0 RQ (task found here 99% of the time)
2. Lock CPU 1 RQ (never finds anything, wastes time)
3. Lock CPU 2 RQ (same)
4. Lock CPU 3 RQ (same)

## Rules

### 1. Always try `this_cpu()` first
Every scheduler function that searches for a task by PID **MUST** try the current
CPU's run queue first before falling back to the all-CPU loop:

```rust
pub fn get_task_foo(pid: Pid) -> Option<Foo> {
    let cpu = this_cpu();
    // Fast path: try current CPU first
    if let Some(result) = with_rq(cpu, |rq| rq.get_task(pid).map(|t| t.foo)).flatten() {
        return Some(result);
    }
    // Slow path: search other CPUs
    for other in 0..num_cpus() {
        if other == cpu { continue; }
        if let Some(result) = with_rq(other, |rq| rq.get_task(pid).map(|t| t.foo)).flatten() {
            return Some(result);
        }
    }
    None
}
```

### 2. `get_current_meta()` uses ultra-fast path
Since the current task is *always* on this CPU's RQ, `get_current_meta()` does a
single `with_rq()` call that gets both the current PID and its meta in one lock:

```rust
pub fn get_current_meta() -> Option<Arc<Mutex<ProcessMeta>>> {
    let cpu = this_cpu();
    with_rq(cpu, |rq| {
        let pid = rq.curr()?;
        rq.get_task(pid).and_then(|t| t.meta.clone())
    }).flatten()
}
```

### 3. Use `try_with_rq` for non-critical queries
Functions like `all_pids()` and `debug_dump_all()` that don't require guaranteed
results should use `try_with_rq` (non-blocking) to avoid contention:
- `all_pids()` → missing a PID on contention is harmless (procfs retries)
- `debug_dump_all()` → already uses try_with_rq (ISR context)

### 4. Use `with_rq` only when result is required
Critical-path functions that **must** succeed still use blocking `with_rq`:
- `pick_next_task()` — scheduler must pick a task
- `block_current()` — must block the task
- `wake_up()` — must wake the task (but NOT from ISR — use `try_wake_up`)

## Performance Impact

Before: Every syscall touching metadata → 4 lock acquisitions (worst case)
After: Every syscall touching metadata → 1 lock acquisition (fast path hit)

This is a **4x reduction in lock contention** for the most common scheduler operations,
directly fixing the `ls /` hang (which does ~50+ syscalls per directory entry).
