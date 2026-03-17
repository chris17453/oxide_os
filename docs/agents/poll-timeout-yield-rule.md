# Poll/Select Timeout: yield_current vs block_current

## Rule
`block_current(TASK_INTERRUPTIBLE)` MUST only be used for **infinite** waits (timeout = -1).
Finite timeouts MUST use `yield_current()` to stay in CFS.

## Why
`block_current` dequeues the task from CFS and sets its state to INTERRUPTIBLE.
The task only runs again when explicitly woken via `try_wake_up()` — typically from
a WaitQueue `wake_all()` call in an fd driver when data arrives.

Timer ticks do NOT re-enqueue INTERRUPTIBLE tasks. They wake the CPU from HLT,
but the scheduler picks a different runnable task. The blocked task's timeout check
code exists but never executes because the task is never scheduled.

Without kernel hrtimer callbacks (which OXIDE OS doesn't have yet), finite timeouts
require the task to remain in CFS so the 100Hz timer tick naturally re-schedules it.

## Correct Pattern
```rust
let use_full_block = timeout_ms < 0; // infinite = safe to fully block

loop {
    // ... signal + timeout checks ...

    if use_full_block {
        sched::block_current(sched_traits::TaskState::TASK_INTERRUPTIBLE);
    } else {
        sched::yield_current(); // stay in CFS — timer tick re-runs us
    }
    os_core::allow_kernel_preempt();
    os_core::wait_for_interrupt();
    os_core::disallow_kernel_preempt();

    // ... re-check fds + re-register on WaitQueues ...
}
```

## Affected Syscalls
- `sys_poll` (poll.rs)
- `sys_select` (poll.rs)
- `sys_pselect6` (poll.rs)
- Any future blocking syscall with timeout (epoll_wait, futex, etc.)

## Performance Note
`yield_current()` at 100Hz means ~10μs scheduler overhead per tick per polling
process. For decisecond-granularity timeouts (ncurses wtimeout, poll with ms args),
this is perfectly fine. If sub-ms precision is needed, implement hrtimer callbacks.

— GraveShift
