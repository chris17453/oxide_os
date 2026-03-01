# Exec Signal Reset Rule

## Rule
On `exec()`, the kernel MUST reset all caught signal handlers (user handlers with
actual addresses) to `SIG_DFL`. Signals set to `SIG_IGN` survive exec unchanged.
Signals already at `SIG_DFL` stay at `SIG_DFL`. Pending signals are cleared.

## Why
After exec, the old process image is gone. Caught handler addresses point into the
old address space — calling them would GPF instantly. Linux does this in
`flush_signal_handlers()` during `do_execve()`.

## Implementation
In `kernel/src/process.rs`, inside the `sys_exec` success path, after updating
`address_space`, `cmdline`, and `environ`:

```rust
for i in 0..signal::NSIG {
    if m.sigactions[i].handler().is_user_handler() {
        m.sigactions[i] = signal::SigAction::new(); // reset to SIG_DFL
    }
}
m.pending_signals = signal::PendingSignals::new(); // fresh start
```

## Impact on Ctrl+C
The shell calls `signal(SIGINT, SIG_IGN)` then forks a child. The child calls
`signal(SIGINT, SIG_DFL)` before exec. Even without the child's explicit reset,
exec now correctly resets caught handlers. SIG_IGN is preserved (correct POSIX
behavior — if the shell sets SIG_IGN, child exec preserves it). The shell's child
explicitly resets to SIG_DFL, so the exec reset is redundant for SIGINT, but it
covers all other signals properly.
