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
- `kernel/syscall/syscall/src/poll.rs` — `check_fd_ready()` MUST use `file.can_read()` (delegates to `poll_read_ready()`). NEVER use `ioctl(FIONREAD)` — it bypasses the ring buffer drain and kills input+signals.

## CRITICAL: check_fd_ready() Must Use file.can_read() (Regression 2026-02-06)

`check_fd_ready()` in `poll.rs` was changed to use `ioctl(FIONREAD)` for CharDevice
readability checks. This completely **bypassed `poll_read_ready()`**, re-introducing
the exact same bug. Symptoms:

- Keyboard input dead in ALL programs using poll() (top, curses-demo, shell)
- Ctrl+C/SIGINT never delivered (signal chars rotted in ring buffer)
- Programs appeared "hung" — blocking on poll() that never returned

The fix: use `file.can_read()` which calls `vnode.poll_read_ready()`. Never
use FIONREAD as the sole readability check for TTYs — it only sees the line
discipline buffer, not the IRQ ring buffer.

**Rule 5: `check_fd_ready()` MUST call `file.can_read()` for ALL file types.**
Never special-case CharDevices with ioctl. The `poll_read_ready()` trait method
exists precisely to handle device-specific readability checks.

## Immediate Signal Delivery (push_input fast-path)

Apps that never call `read()` or `poll()` on stdin (animation loops, render loops)
will never drain the ring buffer. Ctrl+C bytes rot forever, process is unkillable.

Fix: `push_input()` checks for signal characters (0x03/0x1C/0x1A) and delivers
the signal immediately via `SIGNAL_PGRP_CALLBACK`, using `try_lock` for ISR safety.

- Uses `tty.try_isig_enabled()` to respect ISIG termios flag
- Uses `tty.try_get_foreground_pgid()` to avoid blocking on pgid mutex
- Double delivery (here + later in read/poll drain) is harmless — second SIGINT is a no-op

## See Also

- `docs/agents/isr-lock-safety.md` — the ring buffer exists because ISR can't use blocking locks
- `docs/agents/serial-saturation-safety.md` — related I/O safety rules
