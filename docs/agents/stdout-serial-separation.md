# Stdout/Serial Separation Rules

**Author:** GraveShift
**Scope:** `kernel/src/console.rs`, `kernel/vfs/devfs/src/devices.rs`, VT write path

## The Bug (discovered 2026-02-06)

`console_write()` — the function called by the VT TTY driver for ALL stdout output —
was writing EVERY byte to the serial port byte-by-byte before sending it to the
terminal emulator. For curses-demo generating ~28,800 bytes of escape sequences per
frame, this meant:

- 28,800 COM1 spinlock acquire/release cycles per frame
- 28,800 UART THRE checks (port I/O reads)
- 28,800 byte writes (port I/O writes)
- At 115200 baud: ~2.5 seconds per frame just for serial

Result: **0 FPS**. The terminal emulator couldn't render because the write path
was bottlenecked on serial I/O.

## The Fix

`console_write()` now writes ONLY to the terminal emulator. Serial output for
debugging has its own separate path (`os_log`, `debug_*!` macros, `serial_write_bytes()`).

## Rules

### 1. Stdout is NOT debug output
`console_write()` MUST NOT write to serial. User I/O goes to the terminal emulator
only. If you need debug output, use `debug_*!` macros (feature-gated) or `os_log`.

### 2. Serial is for diagnostics only
Serial port writes should only happen via:
- Feature-gated `debug_*!` macros (`debug-all`, `debug-tty-read`, etc.)
- ISR-safe `write_str_unsafe()` / `write_byte_unsafe()` for critical diagnostics
- `os_log::write_str_raw()` for bounded, ISR-safe output
- Explicit `/dev/serial` writes from userspace

### 3. Never add unconditional serial writes to the write path
Any function in the chain: `sys_write` → VFS → ConsoleDevice → VtDevice → Tty →
VtTtyDriver → `console_write()` → `terminal::write()` MUST NOT contain unconditional
serial writes. The entire chain must go straight to the terminal emulator.

### 4. Debug instrumentation must be feature-gated
If you need to trace the write path, use `#[cfg(feature = "debug-tty-read")]` or
similar. NEVER add `println!()` or raw serial writes to the hot path.

## Write Path (clean)

```
write(fd=1, buf, len)
  → sys_write_vfs()
    → file.write()
      → ConsoleDevice::write()
        → VtDevice::write()
          → VtManager::write()
            → Tty::write() (line discipline OPOST)
              → VtTtyDriver::write()
                → console_write(data)
                  → terminal::write(data)  // ONLY destination
                    → TerminalEmulator::write()  // VTE parser, screen buffer
```

No serial. No debug. Just terminal.

## Affected Files

- `kernel/src/console.rs` — `console_write()`, `console_write_bytes()` (serial removed)
- `kernel/vfs/devfs/src/devices.rs` — ConsoleDevice fallback write (serial removed)
- `kernel/syscall/syscall/src/vfs.rs` — Debug statics and serial diag functions removed
- `kernel/src/init.rs` — `set_serial_write()` call removed (dead code)

## See Also

- `docs/agents/serial-saturation-safety.md` — bounded spin rules for serial writes
- `docs/agents/vt-poll-drain.md` — input path (ring buffer drain during poll)
