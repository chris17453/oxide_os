# Blocking Wait CPU Accounting Rules

## The Problem

Kernel blocking syscalls (poll, nanosleep, read) use an `sti; hlt` loop to wait
for events. The process remains `rq.curr()` — it is never dequeued via
`block_current()`. The timer ISR fires every 10ms (TICK_NS) on all CPUs and
`scheduler_tick()` charges `sum_exec_runtime += TICK_NS` for the current task.

Result: every HLT-looping process appears to use ~25% CPU (one full core on a
4-CPU system), even when it's completely idle. `top` shows sshd, rdpd, journald,
shell all at ~24% each — pure fiction.

## The Fix: `kernel_preempt_ok` as Blocking Indicator

The `KERNEL_PREEMPT_OK` per-CPU flag (in `arch-x86_64`) is already set by
blocking syscalls before entering their HLT loop. This flag indicates:

> "I am waiting for an event, not computing. Preempt me freely."

### Scheduler Tick Chain

1. **scheduler.rs** (timer ISR):
   - Reads `kernel_preempt_ok = arch::is_kernel_preempt_allowed()`
   - Non-preemptable kernel path: `sched::scheduler_tick()` (always charges — active computation)
   - Preemptable path: `sched::scheduler_tick_ex(kernel_preempt_ok)` (passes blocking flag)

2. **core.rs** `scheduler_tick_ex(in_blocking_wait: bool)`:
   - If `in_blocking_wait` or idle: classify tick as **idle** in `CPU_TICK_NS`
   - Otherwise: classify as **user** in `CPU_TICK_NS`
   - Passes flag to `rq.scheduler_tick(in_blocking_wait)`

3. **runqueue.rs** `scheduler_tick(in_blocking_wait: bool)`:
   - If `!in_blocking_wait`: charge `sum_exec_runtime += TICK_NS`, update vruntime
   - If `in_blocking_wait`: skip charging (process is waiting, not working)
   - Always: reset `exec_start`, update `min_vruntime`, check preemption

### Rules

1. **NEVER charge CPU time for HLT-looping tasks.** If `kernel_preempt_ok` is true,
   the tick is idle time, not user time.
2. **CPU_TICK_NS accounting must match sum_exec_runtime charging.** If we skip
   charging the task, the tick goes to the idle bucket.
3. **ProcessMeta sync only happens when charging.** The `meta.cpu_time_ns` field
   is only updated when `sum_exec_runtime` actually increases.
4. **vruntime must not advance for blocked tasks.** Otherwise CFS unfairly
   penalizes tasks that spent time waiting.
5. **exec_start must always reset** (even for blocked tasks) to prevent
   `account_stop()` from double-counting when the task finally runs.

### Affected Files

- `kernel/src/scheduler.rs` — timer ISR, calls `scheduler_tick_ex()`
- `kernel/sched/sched/src/core.rs` — `scheduler_tick_ex()`, CPU_TICK_NS accounting
- `kernel/sched/sched/src/runqueue.rs` — `RunQueue::scheduler_tick()`, sum_exec_runtime
- `kernel/sched/sched/src/lib.rs` — re-exports `scheduler_tick_ex`
- `kernel/arch/arch-x86_64/src/lib.rs` — `KERNEL_PREEMPT_OK` flag

### Why Not `block_current()`?

The HLT-based approach is simpler and correct for single-CPU task affinity.
`block_current()` would dequeue the task, requiring explicit `wake_up()` from
the event source (keyboard IRQ, network IRQ, timer). The HLT approach lets the
timer ISR naturally break the loop and re-evaluate. The accounting fix makes
this transparent to userspace — processes only show CPU time they actually used.
