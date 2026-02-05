# VT Poll Drain Rule

## The Bug (discovered 2026-02-05)

Keyboard input was invisible to `poll()` and `select()` on TTY file descriptors.
Ctrl+C (SIGINT) never reached applications using non-blocking input (ncurses nodelay,
`poll(fd=0, POLLIN, timeout=0)`).

## Root Cause: Buffer Split

The VT input pipeline has **two buffers**:

1. **Lock-free ring buffer** (`VtState::input_buffer`) — IRQ handler pushes raw bytes here
2. **Line discipline input queue** (`LineDiscipline::input_queue`) — where `poll()`/`FIONREAD` checks

The ring buffer was only drained into the line discipline during `VtManager::read()`.
But applications call `poll()` FIRST to check if data exists — and `poll()` only
checked the (always-empty) line discipline. Result: **permanent deadlock**.

```
IRQ → ring buffer → (never drained) → line discipline → poll() → "no data"
                                                           ↓
                                                    read() never called
                                                           ↓
                                                    ring buffer never drained
```

## The Fix

`VtDevice::poll_read_ready()` now calls `VtManager::poll_has_input()` which:

1. Drains the ring buffer into the line discipline via `tty.input(&[ch])`
2. Processes signals (Ctrl+C → SIGINT) during the drain
3. Returns `tty.ldisc_can_read()` — true if the ldisc has a readable result

## Rules

1. **`poll_read_ready()` MUST drain the ring buffer** before checking the line discipline.
   Without this, any `poll()`-first pattern (ncurses, select-based I/O) is broken.

2. **Signal delivery happens during drain** — `tty.input()` returns `Some(Signal)` for
   Ctrl+C/Ctrl+\/Ctrl+Z. The drain code MUST call the signal callback. Without this,
   Ctrl+C is dead in any app that uses `poll()` before `read()`.

3. **ConsoleDevice delegates to VtDevice** — `/dev/console`'s `poll_read_ready()` calls
   `backend.poll_read_ready()` which triggers the same drain.

4. **FIONREAD is now consistent** — after the drain, `ldisc.input_available()` returns
   the correct byte count since the bytes have been moved into the input queue.

## Affected Files

- `kernel/tty/vt/src/lib.rs` — `VtManager::poll_has_input()`, `VtDevice::poll_read_ready()`
- `kernel/tty/tty/src/tty.rs` — `Tty::ldisc_can_read()` (new public method)
- `kernel/vfs/devfs/src/devices.rs` — `ConsoleDevice::poll_read_ready()` override
- `kernel/syscall/syscall/src/poll.rs` — `check_fd_ready()` (unchanged, now works correctly)

## See Also

- `docs/agents/isr-lock-safety.md` — the ring buffer exists because ISR can't use blocking locks
- `docs/agents/serial-saturation-safety.md` — related I/O safety rules
