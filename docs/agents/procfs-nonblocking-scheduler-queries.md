# Procfs Non-Blocking Scheduler Queries — SMP Lock Contention Rule

## Rule

All procfs code that queries the scheduler MUST use non-blocking `try_*` variants
of scheduler functions. Using blocking `with_rq()` from procfs causes severe lock
contention on SMP — `top` reads every PID's `/proc/[pid]/stat` and `/proc/[pid]/cmdline`,
each doing up to `num_cpus()` blocking spinlock acquisitions. With 4 CPUs and timer
ISRs also contending for RQ locks, this creates system-wide slowdown and intermittent
lockups.

## Functions

| Blocking (NEVER use from procfs) | Non-blocking (use from procfs) |
|----------------------------------|-------------------------------|
| `get_task_meta(pid)` | `try_get_task_meta(pid)` |
| `get_task_state(pid)` | `try_get_task_state(pid)` |
| `get_task_ppid(pid)` | `try_get_task_ppid(pid)` |
| `get_task_timing_info(pid)` | Already non-blocking (uses `try_with_rq`) |
| `all_pids()` | Already non-blocking (uses `try_with_rq`) |

## Behavior on Contention

When `try_with_rq()` fails (RQ spinlock held by another CPU):
- `try_get_task_meta` → returns `None` → procfs shows empty content for that process
- `try_get_task_state` → returns `None` → process not counted in running/blocked stats
- `try_get_task_ppid` → returns `None` → procfs shows ppid=0
- `try_get_task_meta` + `meta.try_lock()` double gate → returns empty if ProcessMeta contended

This is acceptable for diagnostic output — next read cycle gets fresh data.

## ProcessMeta Lock

`ProcPidCmdline` and `ProcPidStat` both acquire `ProcessMeta` via `Arc<Mutex<...>>`.
Both MUST use `meta.try_lock()` (not `meta.lock()`). Timer ISR signal delivery also
locks ProcessMeta — blocking here causes priority inversion.

## PID List Caching

`ProcRoot::readdir()` uses `PROCFS_PID_CACHE` (global `Mutex<Vec<Pid>>`) to avoid
calling `sched::all_pids()` on every readdir offset. The cache is refreshed when
`pid_idx == 0` (start of each readdir sweep) and reused for subsequent offsets.

Without caching: N processes × 4 CPUs = 4N lock attempts per readdir sweep.
With caching: 4 lock attempts per sweep (at offset 0 only).

## Files

- `kernel/sched/sched/src/core.rs` — `try_get_task_meta`, `try_get_task_state`, `try_get_task_ppid`
- `kernel/vfs/procfs/src/lib.rs` — All procfs callers use `try_*` variants + PID cache
- `kernel/sched/sched/src/lib.rs` — Exports for `try_*` functions
