# CFS Starvation Fixes — Three-Layer Defense

**Author:** GraveShift
**Date:** 2025-05-XX
**Status:** ACTIVE RULE — do not remove these mechanisms

## Problem

CFS tasks could monopolize the CPU indefinitely through three independent starvation vectors:

1. **Sub-tick delta=0 in pick_next_task** — Tasks running for <1 tick (10ms) get `account_stop()` delta=0. Combined with CFS wakeup credit (`min_vruntime - SCHED_LATENCY`), rapid-sleepers (wake → run 1μs → HLT) keep vruntime permanently at the minimum, starving all other tasks.

2. **scheduler_tick not charging vruntime for blocking waits** — Tasks in `in_blocking_wait=true` state (HLT-looping in poll/nanosleep/read) had their vruntime frozen. A task that wakes, runs briefly, and re-enters HLT could accumulate thousands of ticks at the same vruntime.

3. **kpo=0 gate blocking all preemption for long kernel operations** — The `in_kernel && !kernel_preempt_ok` gate prevented ALL context switches for tasks in non-preemptable kernel code. Long-running operations (ext4 mkdir, file create with disk I/O) could hold the CPU for hundreds of milliseconds, starving every other task.

## Fixes

### Fix 1: TICK_NS Floor in pick_next_task (core.rs)

```rust
// In pick_next_task, after account_stop():
if delta == 0 && task.policy.is_fair() {
    delta = TICK_NS; // 10ms floor — one tick granularity
}
task.update_vruntime(delta);
```

**Location:** `kernel/sched/sched/src/core.rs`, `pick_next_task()`
**Rule:** NEVER allow delta=0 for CFS tasks. The minimum charge is one tick period.

### Fix 2: Always Charge Vruntime in scheduler_tick (runqueue.rs)

```rust
// In scheduler_tick, CFS branch:
let delta = sched_traits::TICK_NS;
t.update_vruntime(delta);  // ALWAYS charge vruntime

if !in_blocking_wait {
    t.sum_exec_runtime += delta;  // Only charge CPU time when computing
}
```

**Location:** `kernel/sched/sched/src/runqueue.rs`, `scheduler_tick()`
**Rule:** CFS vruntime advances EVERY tick the task occupies the CPU, regardless of whether it's actively computing or HLT-looping. Only `sum_exec_runtime` (for top/htop CPU%) is gated by `in_blocking_wait`.

### Fix 3: KPO Grace Period (scheduler.rs)

```rust
// In timer ISR, kpo=0 gate:
const KPO_GRACE_TICKS: u32 = 10;  // 100ms grace period

// Track consecutive ticks with kpo=0 for same PID
if streak < KPO_GRACE_TICKS {
    return current_rsp;  // Still within grace period
}
// Grace expired — fall through to forced preemption
```

**Location:** `kernel/src/scheduler.rs`, timer ISR kpo gate
**Rule:** Tasks in non-preemptable kernel code get a 10-tick (100ms) grace period. After that, forced preemption occurs. This prevents long I/O operations from starving all other tasks while giving short critical sections (fork, page table ops) enough time to complete.

## SMP Safety (Fix 4)

Two additional fixes required for `-smp N` (N > 1):

### Fix 4a: GLOBAL_CLOCK BSP-Only Increment

```rust
let now = if cpu == 0 {
    GLOBAL_CLOCK.fetch_add(TICK_NS, Ordering::Relaxed) + TICK_NS
} else {
    GLOBAL_CLOCK.load(Ordering::Relaxed)
};
```

**Location:** `kernel/sched/sched/src/core.rs`, `scheduler_tick_ex()`
**Rule:** Only BSP (CPU 0) advances GLOBAL_CLOCK. With N CPUs all doing fetch_add, the clock runs N× too fast — nanosleep durations, vruntime charging, and all timing calculations are corrupted.

### Fix 4b: Per-CPU KPO Streak Tracking

```rust
const MAX_CPUS: usize = 8;
static KPO_STREAK_PID: [AtomicU64; MAX_CPUS] = ...;
static KPO_STREAK_COUNT: [AtomicU32; MAX_CPUS] = ...;
let cpu_idx = sched::this_cpu() as usize;
```

**Location:** `kernel/src/scheduler.rs`, timer ISR kpo gate
**Rule:** KPO streak tracking MUST be indexed by CPU. Global statics get trashed when multiple CPUs hit the timer ISR simultaneously.

## Safety Guarantees

- **RQ lock deadlock prevention**: The `rq_lock_available()` check ALWAYS runs before context switching, preventing scheduler deadlocks regardless of kpo state.
- **Application lock safety**: Lock waiters (TERMINAL, VFS, etc.) spin with `kpo=1`, so they CAN be preempted — no priority inversion deadlock.
- **Fork/exec safety**: These operations complete in <5 ticks, well within the 10-tick grace period.
- **SMP clock safety**: Only BSP advances GLOBAL_CLOCK. APs read but don't write.

## Verification

Boot with `make run` (SMP 4, 512M). The system should:
- Show ~300 context switches per 5 seconds (perf stats dump)
- All services start (servicemgr, networkd, sshd, journald, etc.)
- Shell responds to input
- `sigtest` passes all 4 signal tests
