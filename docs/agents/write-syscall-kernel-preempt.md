# sys_write Must Enable Kernel Preemption

## Rule

`sys_write_vfs` MUST call `allow_kernel_preempt()` before `file.write()` and
`disallow_kernel_preempt()` after, just like `sys_read_vfs` does.

## Why

The `kernel_preempt_ok` flag is per-CPU. Without this:

1. Shell in `sys_read` (has `allow_kernel_preempt` set) enters `VtManager::read()` loop
2. Shell calls `tty.input()` for echo → acquires `ldisc.lock()` → calls echo callback → acquires `TERMINAL.lock()` (SpinLock)
3. Timer ISR fires — `kernel_preempt_ok=true` → scheduler preempts shell and **clears** `kernel_preempt_ok`
4. Another task (e.g. colors test) runs `sys_write` — no `allow_kernel_preempt` → `kernel_preempt_ok=false`
5. That task spins on `TERMINAL.lock()` — timer ISR fires but `kernel_preempt_ok=false` → scheduler refuses to switch
6. Shell can never resume → `TERMINAL.lock()` never released → **permanent deadlock**

## Symptom

- Random deadlocks in programs that write colored output (ANSI escape sequences)
- Shell login randomly hangs
- Keyboard input still works (ring buffer fills) because PS2 IRQ push is lock-free
- Non-deterministic: depends on timer interrupt firing during echo's `TERMINAL.lock()` critical section

## Fix Location

`kernel/syscall/syscall/src/vfs.rs` — `sys_write_vfs()`:

```rust
// Allow kernel preemption — prevents deadlock if TERMINAL.lock() is held by preempted task
unsafe {
    if let Some(f) = (*addr_of!(super::SYSCALL_CONTEXT)).allow_kernel_preempt { f(); }
}
// ... stac ... file.write(buffer) ... clac ...
unsafe {
    if let Some(f) = (*addr_of!(super::SYSCALL_CONTEXT)).disallow_kernel_preempt { f(); }
}
```

## General Rule

Any syscall that may spin on a kernel SpinLock held by another task MUST enable
kernel preemption. Without it, the spinning task can never be preempted to let
the lock holder run. This applies to: write, ioctl, any VFS operation that
reaches `TERMINAL.lock()`, `ldisc.lock()`, or any other contended spinlock.
