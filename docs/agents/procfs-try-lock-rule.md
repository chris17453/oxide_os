# Procfs MUST Use try_lock for ProcessMeta

**Author:** GraveShift
**Status:** ACTIVE RULE — never use blocking lock() in procfs generate_content

## Problem

Procfs `generate_content()` used blocking `meta.lock()` on ProcessMeta. When the
timer ISR held the same ProcessMeta for signal delivery (via `try_lock()`), the ISR
correctly skipped it — but the procfs path would spin on the lock. More critically,
when `top` reads `/proc/<pid>/stat` for every process in a tight loop, blocking locks
cause priority inversion: top holds the CPU spinning on a lock that another task needs
to release, but that task can't run because top holds the CPU.

Additionally, `generate_content()` calls `sched::get_task_timing_info()` while holding
the ProcessMeta lock, introducing a nested lock pattern (ProcessMeta → RQ lock).

## Fix

Replace `meta.lock()` with `meta.try_lock()` in all procfs `generate_content()`
implementations. On contention, return an empty string — the next `read()` call
gets fresh data.

```rust
fn generate_content(&self) -> String {
    if let Some(meta) = sched::get_task_meta(self.pid) {
        let m = match meta.try_lock() {
            Some(guard) => guard,
            None => return String::new(),
        };
        // ... format process info ...
    } else {
        String::new()
    }
}
```

**Location:** `kernel/vfs/procfs/src/lib.rs`, `ProcPidStat::generate_content()` and
`ProcPidStatus::generate_content()`.

## Rules

1. **Procfs `generate_content()` MUST use `try_lock()` for ProcessMeta.** Blocking
   `lock()` causes priority inversion when `top` or similar tools read `/proc`.

2. **Return empty string on lock contention.** The procfs file simply appears empty
   for that read — the application retries on the next cycle. For `top`, this means
   one process might be momentarily missing from the display.

3. **Never call blocking scheduler functions while holding ProcessMeta.** The pattern
   `meta.lock()` → `get_task_timing_info()` → `with_rq()` creates a nested lock
   chain. If the RQ lock is contended, the entire chain blocks.

## sys_getdents Preemption

`sys_getdents` MUST enable kernel preemption (`kpo=1`) during directory iteration.
Procfs readdir calls `sched::all_pids()` which locks all CPU RQs. Without kpo=1,
the entire iteration runs non-preemptably, and the KPO grace period (100ms) is the
only escape valve.

**Location:** `kernel/syscall/syscall/src/dir.rs`, `sys_getdents()`.

```rust
// Enable kpo before the readdir loop
allow_kernel_preempt();

loop {
    let entry = file.vnode().readdir(offset);
    // ... write to user buffer ...
}

// Restore kpo after the loop
disallow_kernel_preempt();
```
