# Rule: Task Context Must Be Set Before Enqueue

**Severity:** Critical
**Subsystem:** Scheduler, Process Creation

## The Rule

When creating a new task (fork, clone, init, or any future path), **set `task.context`** on the `Task` struct **BEFORE** calling `sched::add_task()`.

Never use this pattern:
```rust
// BAD — race window between add_task and set_task_context
let child_task = sched::Task::new_with_meta(...);
sched::add_task(child_task);                         // schedulable with cs=0
sched::set_task_context(child_pid, child_task_ctx);  // too late
```

Always use this pattern:
```rust
// GOOD — context is baked in before scheduler sees it
let mut child_task = sched::Task::new_with_meta(...);
child_task.context = child_task_ctx;  // set BEFORE enqueue
sched::add_task(child_task);
```

## Why

With 4 CPUs at 100Hz, that's a 400Hz lottery for hitting the microsecond race window between `add_task()` and `set_task_context()`. When a CPU's timer tick picks up a task with `cs=0, ss=0, rip=0`, the iretq causes a GPF or page fault. This manifests as intermittent crashes that depend on timer timing — the exact "random crash" pattern.

## Layered Defense

1. **Primary fix:** Set context before enqueue (eliminates the race)
2. **Safe default:** `TaskContext::default()` uses `cs=0x08, ss=0x10, rflags=0x202` — valid kernel selectors that won't GPF on segment load. `rip=0` and `rsp=0` are sentinels.
3. **Validation:** `context_switch_transaction()` calls `is_schedulable()` — if `rip=0` or `rsp=0`, bail and retry next tick.

## Files

- `kernel/src/process.rs` — fork and clone paths
- `kernel/src/init.rs` — init task creation
- `kernel/sched/sched/src/task.rs` — `TaskContext::default()`, `is_schedulable()`
- `kernel/sched/sched/src/core.rs` — validation in `context_switch_transaction()`
