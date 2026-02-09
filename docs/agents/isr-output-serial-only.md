# ISR-Safe Output Must Go to Serial Only

— GraveShift: The rule born from the great "kernel authenticates itself" incident.

## The Rule

**ISR-safe debug output (`os_log::write_str_raw`, `println_unsafe!`) MUST write to the serial port ONLY. Never to the terminal, never to the VT input buffer.**

## The Bug That Taught Us

`console::write_byte_unsafe()` called `manager.push_input(byte)` — shoving every byte of ISR debug output into the VT **input** ring buffer. The same buffer that keyboard events go into.

When the kernel wrote `[INFO] Starting APIC timer at 100Hz...\n` during boot, those bytes landed in the VT input queue. Getty was reading from that queue, waiting for a username. It received:
- `[INFO] Starting APIC timer at 100Hz...` → Login incorrect
- `[APIC-CAL] Starting calibration...` → Login incorrect
- `[APIC-CAL] port61 initial: 0x21` → Login incorrect
- → "Too many failed attempts"

The user never typed a single character. The kernel logged itself in (and failed).

## Call Chain (Fixed)

```
os_log::write_str_raw("debug msg")
  → UNSAFE_WRITE_STR function pointer
    → console::write_str_unsafe()
      → arch::serial::write_str_unsafe()    ← CORRECT: goes to COM1
        → serial::write_byte_unsafe()       ← bounded spin (2048 max)
          → outb(COM1 + DATA, byte)         ← hardware port I/O
```

## Call Chain (Broken — What We Fixed)

```
os_log::write_str_raw("debug msg")
  → UNSAFE_WRITE_STR function pointer
    → console::write_str_unsafe()
      → console::write_byte_unsafe()
        → manager.push_input(byte)          ← WRONG: VT INPUT buffer!
          → getty reads it as keyboard input ← catastrophe
```

## Why Not Terminal Output?

The ISR-safe path exists because we CAN'T acquire the terminal lock (deadlock risk). Options were:
1. Write to serial (lock-free port I/O) ← chosen
2. Use `terminal::try_lock()` and drop on contention ← loses debug output

Serial is the correct choice. Debug output belongs on the wire. User-visible output goes through `console_write()` → `terminal::write()` (locking path, process context only).

## Output Path Summary

| Function | Destination | Lock | Context | Use For |
|----------|-------------|------|---------|---------|
| `console_write()` | Terminal framebuffer | Mutex | Process | Stdout/user output |
| `console::write_str_unsafe()` | Serial COM1 | None | ISR/Any | Debug spew |
| `os_log::println!()` | Serial + Terminal | Mutex | Process | Boot messages |
| `os_log::println_unsafe!()` | Serial COM1 | None | ISR/Any | ISR debug |
| `manager.push_input()` | VT input ring | Lock-free | ISR | Keyboard/mouse ONLY |

## Prevention

- `push_input()` is for keyboard scancodes, serial stdin bytes, and mouse events ONLY
- NEVER call `push_input()` with debug/log/status output
- If you need ISR-safe output, use `os_log::write_str_raw()` — it goes to serial
- If you need user-visible output from process context, use `console_write()`

---

— GraveShift: The kernel should not type on its own behalf. That's the user's job.
