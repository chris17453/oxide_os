# Signal Delivery in Blocking Kernel Loops

## Rule
Every kernel blocking loop (VT read, waitpid, pipe read, sleep, poll) MUST check for
pending actionable signals and return -EINTR when a signal should interrupt the operation.
Without this, signals get queued but never delivered because the process never returns from
the syscall to trigger `check_signals_on_syscall_return()`.

## Why This Matters
In Linux, blocking functions use `wait_event_interruptible()` which checks `signal_pending()`
on every wakeup. OXIDE must do the same. A process stuck in a blocking kernel loop with a
pending SIGINT is effectively unkillable — Ctrl+C does nothing.

## How to Check
Use `signal::delivery::should_interrupt_for_signal()` which checks:
1. Are there pending signals? (not just queued — deliverable)
2. Are they blocked? (signal mask)
3. Are they ignored? (SIG_IGN or default=Ignore like SIGCHLD)

Only returns true for signals that would actually DO something (terminate, stop, invoke handler).
This prevents the shell (which ignores SIGINT) from getting spuriously interrupted.

## Pattern for Blocking Loops
```rust
// In the blocking loop, after yield/HLT:
if let Some(meta_arc) = sched::get_task_meta(current_pid) {
    if let Some(meta) = meta_arc.try_lock() {
        if signal::delivery::should_interrupt_for_signal(
            &meta.pending_signals.set(),
            &meta.signal_mask,
            &meta.sigactions,
        ) {
            return Err(VfsError::Interrupted); // or -4 (EINTR)
        }
    }
}
```

For VT reads, a callback (`SignalPendingFn`) is used to avoid circular crate dependencies.

## Signal Delivery MUST Wake Sleeping Processes

**CRITICAL:** Sending a signal is NOT just queuing it in `pending_signals`. The sender MUST also
wake the target process if it's in TASK_INTERRUPTIBLE (sleeping in nanosleep, blocking read, etc.).

In Linux: `complete_signal()` → `signal_wake_up()` → `wake_up_state(TASK_INTERRUPTIBLE)`.

In OXIDE: After `meta.send_signal(sig, info)`, call `sched::wake_up(pid)` (syscall context) or
`sched::try_wake_up(pid)` (ISR context). Without this, the signal sits in `pending_signals` but
the process never notices because it's sleeping through `sti; hlt` and nobody re-enqueues it on
the run queue.

### Call sites that deliver signals:
- `kill_pgrp()` in `kernel/src/init.rs` — uses `try_wake_up()` (may be ISR context from push_input)
- `send_signal_to_pid()` in `kernel/syscall/syscall/src/signal.rs` — uses `wake_up()` (syscall ctx)
- `send_signal_to_pgrp()` in `kernel/syscall/syscall/src/signal.rs` — uses `wake_up()` (syscall ctx)

### ISR Lock Safety for kill_pgrp
`kill_pgrp()` is called from `signal_pgrp_callback` which can run from IRQ context (push_input
Ctrl+C fast path). It MUST use `try_lock()` on ProcessMeta, not `.lock()`. Using `.lock()` from
ISR context deadlocks if any other ProcessMeta lock is held on the same CPU.

## Locations Fixed
- `kernel/tty/vt/src/lib.rs` — VtManager::read() checks via SIGNAL_PENDING_CALLBACK
- `kernel/src/process.rs` — kernel_wait() checks after HLT wakeup
- `kernel/vfs/vfs/src/error.rs` — VfsError::Interrupted maps to -4 (EINTR)
- `kernel/src/init.rs` — kill_pgrp() wakes processes + uses try_lock for ISR safety
- `kernel/syscall/syscall/src/signal.rs` — send_signal_to_pid/pgrp wake sleeping targets

## Related
- `docs/agents/write-syscall-kernel-preempt.md` — preemption in blocking syscalls
- `docs/agents/isr-lock-safety.md` — try_lock patterns in signal delivery
