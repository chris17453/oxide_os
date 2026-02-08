# /dev/kmsg Lock-Free Requirement

## Rule

**NEVER acquire scheduler locks from `/dev/kmsg` write path callbacks.**

## Problem

The `/dev/kmsg` device is used for kernel and service logging. When a userspace process writes to `/dev/kmsg`, the kernel callbacks (`kmsg_get_proc_name`, `kmsg_get_pid`, `kmsg_get_uptime_ms`) are invoked to stamp the log entry with metadata.

If these callbacks acquire **any scheduler locks** (like `ProcessMeta` mutex), a deadlock can occur:

1. **Userspace thread** writes to stdout → `/dev/kmsg`
2. `kmsg::process_write()` calls `kmsg_get_proc_name(pid)`
3. Callback acquires `meta.lock()` on `Arc<Mutex<ProcessMeta>>`
4. **Timer interrupt fires** while holding that lock
5. Timer ISR tries to reschedule, needs task metadata
6. ISR tries to acquire the **same lock**
7. **DEADLOCK** - ISR spins forever waiting for lock held by interrupted thread

## Symptoms

- System lockup when service manager or services write to `/dev/kmsg`
- Service manager redirects all service stdout/stderr to `/dev/kmsg`
- System boots fine without service manager, locks up with it enabled
- High context switch count but zero terminal renders (services are running but output is stuck)

## Solution

All `/dev/kmsg` callbacks registered in `kernel/src/init.rs` must be **lock-free**:

- `kmsg_get_pid()` - ✅ Uses `current_pid_lockfree()` (already lock-free)
- `kmsg_get_uptime_ms()` - ✅ Reads atomic timer ticks (already lock-free)
- `kmsg_get_proc_name()` - ❌ **FIXED** - Now returns empty (uses "unknown" tag)

### Current Implementation

```rust
fn kmsg_get_proc_name(_pid: u32, _buf: &mut [u8]) -> usize {
    // — GraveShift: DISABLED to prevent deadlock
    // Original code called get_task_meta(pid).lock() which can deadlock with timer ISR
    // TODO: Implement lock-free per-CPU process name storage for proper kmsg tagging
    0
}
```

### Future Enhancement

To properly tag `/dev/kmsg` entries with process names, implement:

1. **Per-CPU process name storage** (like per-CPU PID)
2. Updated during context switch: `set_current_name_lockfree(cpu, name)`
3. Read without locks: `current_name_lockfree() -> &'static str`

## Related

- `docs/agents/isr-lock-safety.md` - ISR lock-free requirements
- `docs/agents/println-with-cli-deadlock.md` - General interrupt-lock deadlocks
- `kernel/vfs/devfs/src/kmsg.rs` - `/dev/kmsg` implementation
- `kernel/src/init.rs:2089` - Callback registration

— GraveShift: When you're debugging a lockup and journald is your only witness, don't let journald be the crime scene
